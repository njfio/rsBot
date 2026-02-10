use std::{
    fs,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use httpmock::prelude::*;
use predicates::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct SessionEntry {
    id: u64,
    parent_id: Option<u64>,
    message: SessionMessage,
}

#[derive(Debug, Deserialize)]
struct SessionMessage {
    role: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
enum SessionRecord {
    Meta {
        schema_version: u32,
    },
    Entry {
        id: u64,
        parent_id: Option<u64>,
        message: SessionMessage,
    },
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RpcSchemaCompatMode {
    DispatchNdjson,
    ServeNdjson,
}

#[derive(Debug, Deserialize, PartialEq)]
struct RpcSchemaCompatFixture {
    schema_version: u32,
    name: String,
    mode: RpcSchemaCompatMode,
    input_lines: Vec<String>,
    expected_processed_lines: usize,
    expected_error_count: usize,
    expected_responses: Vec<Value>,
}

fn parse_session_entries(path: &std::path::Path) -> Vec<SessionEntry> {
    let raw = fs::read_to_string(path).expect("session file should exist");
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let record = serde_json::from_str::<SessionRecord>(line).expect("line should parse");
            match record {
                SessionRecord::Meta { schema_version } => {
                    assert_eq!(schema_version, 1);
                    None
                }
                SessionRecord::Entry {
                    id,
                    parent_id,
                    message,
                } => Some(SessionEntry {
                    id,
                    parent_id,
                    message,
                }),
            }
        })
        .collect()
}

fn binary_command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("tau-coding-agent"))
}

fn write_model_catalog(path: &std::path::Path, entries: serde_json::Value) {
    let payload = json!({
        "schema_version": 1,
        "entries": entries,
    });
    fs::write(path, format!("{payload}\n")).expect("write model catalog");
}

fn rpc_schema_compat_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("rpc-schema-compat")
        .join(name)
}

fn load_rpc_schema_compat_fixture(name: &str) -> RpcSchemaCompatFixture {
    let path = rpc_schema_compat_fixture_path(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let fixture = serde_json::from_str::<RpcSchemaCompatFixture>(&raw)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
    assert_eq!(
        fixture.schema_version,
        1,
        "unsupported fixture schema_version in {}",
        path.display()
    );
    fixture
}

fn parse_ndjson_values(raw: &str) -> Vec<Value> {
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("line should parse as JSON"))
        .collect::<Vec<_>>()
}

#[test]
fn help_hides_environment_variable_values() {
    let mut cmd = binary_command();
    cmd.arg("--help")
        .env("OPENAI_API_KEY", "SUPER_SECRET_TEST_TOKEN_123")
        .env("ANTHROPIC_API_KEY", "SUPER_SECRET_ANTHROPIC_456");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("OPENAI_API_KEY"))
        .stdout(predicate::str::contains("ANTHROPIC_API_KEY"))
        .stdout(predicate::str::contains("SUPER_SECRET_TEST_TOKEN_123").not())
        .stdout(predicate::str::contains("SUPER_SECRET_ANTHROPIC_456").not());
}

#[test]
fn no_session_and_branch_from_combination_fails_fast() {
    let mut cmd = binary_command();
    cmd.args(["--no-session", "--branch-from", "1", "--prompt", "hello"]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "--branch-from cannot be used together with --no-session",
    ));
}

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
fn session_validate_flag_succeeds_for_valid_session_file() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"sys"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"hello"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write valid session");

    let mut cmd = binary_command();
    cmd.args([
        "--session",
        session.to_str().expect("utf8 path"),
        "--session-validate",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session validation passed"))
        .stdout(predicate::str::contains("entries=2"));
}

#[test]
fn regression_session_validate_flag_fails_for_invalid_session_file() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":2,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"sys"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"cycle"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write invalid session");

    let mut cmd = binary_command();
    cmd.args([
        "--session",
        session.to_str().expect("utf8 path"),
        "--session-validate",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("session validation failed"))
        .stderr(predicate::str::contains("cycles=2"));
}

#[test]
fn regression_qa_loop_flag_takes_preflight_precedence_over_prompt() {
    let temp = tempdir().expect("tempdir");
    let config_path = temp.path().join("qa-loop.json");
    fs::write(
        &config_path,
        format!(
            "{}\n",
            json!({
                "schema_version": 1,
                "stages": [
                    {"name": "smoke", "command": "echo qa-loop-preflight"}
                ]
            })
        ),
    )
    .expect("write qa-loop config");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path()).args([
        "--qa-loop",
        "--qa-loop-config",
        config_path.to_str().expect("utf8 path"),
        "--prompt",
        "ignored prompt",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("qa-loop summary: outcome=pass"))
        .stdout(predicate::str::contains(
            "qa-loop stage: name=smoke status=pass",
        ))
        .stdout(predicate::str::contains(
            "qa-loop attempt stdout: stage=smoke",
        ));
}

#[test]
fn regression_qa_loop_flag_returns_failure_with_json_root_cause() {
    let temp = tempdir().expect("tempdir");
    let config_path = temp.path().join("qa-loop.json");
    fs::write(
        &config_path,
        format!(
            "{}\n",
            json!({
                "schema_version": 1,
                "stages": [
                    {"name": "failing", "command": "echo qa-loop-failed 1>&2; exit 3"}
                ]
            })
        ),
    )
    .expect("write qa-loop config");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path()).args([
        "--qa-loop",
        "--qa-loop-config",
        config_path.to_str().expect("utf8 path"),
        "--qa-loop-json",
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("\"outcome\":\"fail\""))
        .stdout(predicate::str::contains("\"root_cause_stage\":\"failing\""))
        .stderr(predicate::str::contains(
            "qa-loop failed: root_cause_stage=failing",
        ));
}

#[test]
fn interactive_help_and_unknown_command_suggestions_work() {
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
    ])
    .write_stdin("/help\n/help branch\n/help polciy\n/polciy\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("commands:"))
        .stdout(predicate::str::contains("usage: /branch <id>"))
        .stdout(predicate::str::contains("unknown help topic: /polciy"))
        .stdout(predicate::str::contains("did you mean /policy?"));
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
fn integration_models_list_command_filters_catalog_entries() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    write_model_catalog(
        &catalog_path,
        json!([
            {
                "provider": "openai",
                "model": "gpt-4o-mini",
                "context_window_tokens": 128000,
                "supports_tools": true,
                "supports_multimodal": true,
                "supports_reasoning": true,
                "input_cost_per_million": 0.15,
                "output_cost_per_million": 0.6
            },
            {
                "provider": "openai",
                "model": "legacy-no-tools",
                "context_window_tokens": 8192,
                "supports_tools": false,
                "supports_multimodal": false,
                "supports_reasoning": false,
                "input_cost_per_million": null,
                "output_cost_per_million": null
            }
        ]),
    );

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--model-catalog-offline",
        "--no-session",
    ])
    .write_stdin("/models-list gpt --provider openai --tools true --limit 5\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("models list: source=cache:"))
        .stdout(predicate::str::contains("model: openai/gpt-4o-mini"))
        .stdout(predicate::str::contains("legacy-no-tools").not());
}

#[test]
fn regression_model_show_command_reports_not_found_and_continues() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    write_model_catalog(
        &catalog_path,
        json!([{
            "provider": "openai",
            "model": "gpt-4o-mini",
            "context_window_tokens": 128000,
            "supports_tools": true,
            "supports_multimodal": true,
            "supports_reasoning": true,
            "input_cost_per_million": 0.15,
            "output_cost_per_million": 0.6
        }]),
    );

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--model-catalog-offline",
        "--no-session",
    ])
    .write_stdin("/model-show openai/missing-model\n/help model-show\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("model show: not found"))
        .stdout(predicate::str::contains("command: /model-show"));
}

#[test]
fn integration_startup_model_catalog_remote_refresh_is_reported() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    let server = MockServer::start();
    let refresh = server.mock(|when, then| {
        when.method(GET).path("/models.json");
        then.status(200).json_body(json!({
            "schema_version": 1,
            "entries": [{
                "provider": "openai",
                "model": "gpt-4o-mini",
                "context_window_tokens": 128000,
                "supports_tools": true,
                "supports_multimodal": true,
                "supports_reasoning": true,
                "input_cost_per_million": 0.15,
                "output_cost_per_million": 0.6
            }]
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--model-catalog-url",
        &format!("{}/models.json", server.base_url()),
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "model catalog: source=remote url=",
        ))
        .stdout(predicate::str::contains("entries=1"));
    refresh.assert_calls(1);
}

#[test]
fn regression_startup_rejects_tool_incompatible_model_from_catalog() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    write_model_catalog(
        &catalog_path,
        json!([{
            "provider": "openai",
            "model": "no-tools-model",
            "context_window_tokens": 8192,
            "supports_tools": false,
            "supports_multimodal": false,
            "supports_reasoning": false,
            "input_cost_per_million": null,
            "output_cost_per_million": null
        }]),
    );

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/no-tools-model",
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--model-catalog-offline",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("tool-incompatible"));
}

#[test]
fn integration_prompt_plan_first_mode_emits_trace_and_executes_two_phases() {
    let server = MockServer::start();
    let planner = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-max-plan-steps",
        "4",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=planner approved_steps=2",
        ))
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=executor",
        ));

    planner.assert_calls(2);
}

#[test]
fn integration_prompt_plan_first_delegate_steps_emits_delegation_trace() {
    let server = MockServer::start();
    let planner_delegate_consolidation = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-delegate-steps",
        "--orchestrator-max-plan-steps",
        "4",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=executor strategy=delegated-steps total_steps=2",
        ))
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=delegated-step step=1 action=start",
        ))
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=delegated-step step=2 action=complete",
        ))
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=consolidation delegated_steps=2",
        ));

    planner_delegate_consolidation.assert_calls(4);
}

#[test]
fn regression_prompt_plan_first_delegate_steps_fail_on_step_count_budget_overrun() {
    let server = MockServer::start();
    let planner_delegate = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-delegate-steps",
        "--orchestrator-max-plan-steps",
        "4",
        "--orchestrator-max-delegated-steps",
        "1",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=delegated-step decision=reject reason=delegated_step_count_budget_exceeded",
        ))
        .stderr(predicate::str::contains("delegated step budget exceeded"));

    planner_delegate.assert_calls(1);
}

#[test]
fn regression_prompt_plan_first_delegate_steps_fails_on_budget_overrun() {
    let server = MockServer::start();
    let planner_delegate_consolidation = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-delegate-steps",
        "--orchestrator-max-plan-steps",
        "4",
        "--orchestrator-max-executor-response-chars",
        "12",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=consolidation decision=reject reason=executor_response_budget_exceeded",
        ))
        .stderr(predicate::str::contains(
            "consolidation response exceeded budget",
        ));

    planner_delegate_consolidation.assert_calls(4);
}

#[test]
fn regression_prompt_plan_first_delegate_steps_fail_on_step_budget_overrun() {
    let server = MockServer::start();
    let planner_delegate = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-delegate-steps",
        "--orchestrator-max-plan-steps",
        "4",
        "--orchestrator-max-delegated-step-response-chars",
        "12",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=delegated-step step=1 decision=reject reason=delegated_step_response_budget_exceeded",
        ))
        .stderr(predicate::str::contains(
            "delegated step 1 response exceeded budget",
        ));

    planner_delegate.assert_calls(2);
}

#[test]
fn regression_prompt_plan_first_delegate_steps_fail_on_total_budget_overrun() {
    let server = MockServer::start();
    let planner_delegate = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-delegate-steps",
        "--orchestrator-max-plan-steps",
        "4",
        "--orchestrator-max-delegated-step-response-chars",
        "80",
        "--orchestrator-max-delegated-total-response-chars",
        "70",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=delegated-step step=2 decision=reject reason=delegated_total_response_budget_exceeded",
        ))
        .stderr(predicate::str::contains(
            "delegated responses exceeded cumulative budget",
        ));

    planner_delegate.assert_calls(3);
}

#[test]
fn regression_prompt_plan_first_mode_fails_closed_on_overlong_plan() {
    let server = MockServer::start();
    let planner = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Step one\n2. Step two\n3. Step three"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-max-plan-steps",
        "2",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("planner produced 3 steps"));

    planner.assert_calls(1);
}

#[test]
fn regression_prompt_plan_first_mode_fails_when_executor_response_exceeds_budget() {
    let server = MockServer::start();
    let planner_and_executor = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "prepare release plan",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-max-plan-steps",
        "4",
        "--orchestrator-max-executor-response-chars",
        "12",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=consolidation decision=reject reason=executor_response_budget_exceeded",
        ))
        .stdout(
            predicate::str::contains(
                "orchestrator trace: mode=plan-first phase=consolidation decision=accept",
            )
            .not(),
        )
        .stderr(predicate::str::contains("executor response exceeded budget"));

    planner_and_executor.assert_calls(2);
}

#[test]
fn integration_interactive_plan_first_mode_runs_planner_and_executor_per_turn() {
    let server = MockServer::start();
    let planner = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Inspect requirements\n2. Apply implementation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-max-plan-steps",
        "4",
        "--no-session",
    ])
    .write_stdin("interactive request\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=planner approved_steps=2",
        ))
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=executor",
        ));

    planner.assert_calls(2);
}

#[test]
fn regression_interactive_plan_first_mode_overlong_plan_fails_before_executor() {
    let server = MockServer::start();
    let planner = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "1. Step one\n2. Step two\n3. Step three"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 6, "total_tokens": 16}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-max-plan-steps",
        "2",
        "--no-session",
    ])
    .write_stdin("interactive request\n");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("planner produced 3 steps"));

    planner.assert_calls(1);
}

#[test]
fn integration_interactive_session_search_command_finds_results_across_branches() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-search.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"main target"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":3,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"branch target"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-search target\n/quit\n");

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("stdout should be utf8");
    assert!(stdout.contains("session search: query=\"target\""));
    let main_index = stdout.find("result: id=2").expect("main result");
    let branch_index = stdout.find("result: id=3").expect("branch result");
    assert!(main_index < branch_index);
}

#[test]
fn regression_interactive_session_search_command_empty_query_prints_usage_and_continues() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-search-empty.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-search\n/help session-search\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("usage: /session-search <query>"))
        .stdout(predicate::str::contains("command: /session-search"))
        .stdout(predicate::str::contains("usage: /session-search <query>"));
}

#[test]
fn integration_interactive_session_stats_command_reports_branched_summary() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-stats.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"main"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":3,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"branch"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-stats\n/session-stats --json\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "session stats: entries=3 branch_tips=2 roots=1 max_depth=2",
        ))
        .stdout(predicate::str::contains(
            "heads: active=3 latest=3 active_is_latest=true",
        ))
        .stdout(predicate::str::contains("depth: active=2 latest=2"))
        .stdout(predicate::str::contains("role: system=1"))
        .stdout(predicate::str::contains("role: user=2"))
        .stdout(predicate::str::contains("\"entries\":3"))
        .stdout(predicate::str::contains("\"branch_tips\":2"))
        .stdout(predicate::str::contains("\"role_counts\""))
        .stdout(predicate::str::contains("\"user\":2"));
}

#[test]
fn regression_interactive_session_stats_command_with_args_prints_usage_and_continues() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-stats-usage.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-stats extra\n/help session-stats\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("usage: /session-stats [--json]"))
        .stdout(predicate::str::contains("command: /session-stats"))
        .stdout(predicate::str::contains("usage: /session-stats [--json]"));
}

#[test]
fn integration_interactive_session_diff_command_reports_shared_and_divergent_lineage() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-diff.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"main"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":3,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"branch"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/branch 2\n/session-diff\n/session-diff 2 3\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("switched to branch id 2"))
        .stdout(predicate::str::contains(
            "session diff: source=default left=2 right=3",
        ))
        .stdout(predicate::str::contains(
            "summary: shared_depth=1 left_depth=2 right_depth=2 left_only=1 right_only=1",
        ))
        .stdout(predicate::str::contains(
            "shared: id=1 parent=none role=system preview=root",
        ))
        .stdout(predicate::str::contains(
            "left-only: id=2 parent=1 role=user preview=main",
        ))
        .stdout(predicate::str::contains(
            "right-only: id=3 parent=1 role=user preview=branch",
        ))
        .stdout(predicate::str::contains(
            "session diff: source=explicit left=2 right=3",
        ));
}

#[test]
fn regression_interactive_session_diff_command_with_args_prints_usage_and_continues() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-diff-usage.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-diff 1\n/help session-diff\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "usage: /session-diff [<left-id> <right-id>]",
        ))
        .stdout(predicate::str::contains("command: /session-diff"))
        .stdout(predicate::str::contains(
            "usage: /session-diff [<left-id> <right-id>]",
        ));
}

#[test]
fn regression_interactive_session_diff_command_unknown_ids_are_reported() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-diff-unknown.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-diff 999 1\n/quit\n");

    cmd.assert().success().stdout(predicate::str::contains(
        "session diff error: unknown left session id 999",
    ));
}

#[test]
fn integration_interactive_doctor_command_reports_runtime_diagnostics() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("doctor-session.jsonl");
    let skills_dir = temp.path().join("skills");
    let lock_path = temp.path().join("skills.lock.json");
    let trust_root_path = temp.path().join("trust-roots.json");
    let tau_root = temp.path().join(".tau");
    let ingress_dir = tau_root.join("multi-channel/live-ingress");
    let credential_store = tau_root.join("credentials.json");
    fs::create_dir_all(&skills_dir).expect("mkdir skills");
    fs::create_dir_all(&ingress_dir).expect("mkdir ingress");
    fs::write(skills_dir.join("focus.md"), "focus skill").expect("write skill");
    fs::write(&lock_path, "{}\n").expect("write lock");
    fs::write(&trust_root_path, "[]\n").expect("write trust roots");
    fs::write(ingress_dir.join("telegram.ndjson"), "").expect("write telegram inbox");
    fs::write(ingress_dir.join("discord.ndjson"), "").expect("write discord inbox");
    fs::write(ingress_dir.join("whatsapp.ndjson"), "").expect("write whatsapp inbox");
    fs::write(
        &credential_store,
        "{\"schema_version\":1,\"encryption\":\"none\",\"providers\":{},\"integrations\":{}}\n",
    )
    .expect("write credential store");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--session",
            session.to_str().expect("utf8 path"),
            "--skills-dir",
            skills_dir.to_str().expect("utf8 path"),
            "--skills-lock-file",
            lock_path.to_str().expect("utf8 path"),
            "--skill-trust-root-file",
            trust_root_path.to_str().expect("utf8 path"),
        ])
        .env("TAU_TELEGRAM_BOT_TOKEN", "telegram-token")
        .env("TAU_DISCORD_BOT_TOKEN", "discord-token")
        .env("TAU_WHATSAPP_ACCESS_TOKEN", "whatsapp-access-token")
        .env("TAU_WHATSAPP_PHONE_NUMBER_ID", "15551234567")
        .write_stdin("/doctor\n/doctor --json\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "doctor summary: checks=16 pass=13 warn=3 fail=0",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=release_channel status=pass code=default_stable",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=release_update status=warn code=skipped_offline",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=provider_auth_mode.openai status=pass code=api_key",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=provider_key.openai status=pass code=present",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=session_path status=pass code=readable",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=skills_lock status=pass code=readable",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=trust_root status=pass code=readable",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=multi_channel_live.ingress_dir status=pass code=ready",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=multi_channel_live.channel_policy status=warn code=missing",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=multi_channel_live.channel_policy.risk status=warn code=unknown_without_policy_file",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=multi_channel_live.channel.telegram status=pass code=ready",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=multi_channel_live.channel.discord status=pass code=ready",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=multi_channel_live.channel.whatsapp status=pass code=ready",
        ))
        .stdout(predicate::str::contains("\"summary\""))
        .stdout(predicate::str::contains("\"checks\""))
        .stdout(predicate::str::contains("\"provider_auth_mode.openai\""))
        .stdout(predicate::str::contains("\"provider_key.openai\""))
        .stdout(predicate::str::contains(
            "\"multi_channel_live.channel.telegram\"",
        ));
}

#[test]
fn regression_interactive_doctor_command_with_args_prints_usage_and_continues() {
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
    ])
    .write_stdin("/doctor --bad\n/help doctor\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "usage: /doctor [--json] [--online]",
        ))
        .stdout(predicate::str::contains("command: /doctor"))
        .stdout(predicate::str::contains("example: /doctor"));
}

#[test]
fn integration_interactive_session_graph_export_command_writes_mermaid_file() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-graph-export.jsonl");
    let graph_path = temp.path().join("session-graph.mmd");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"child"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin(format!(
        "/session-graph-export {}\n/quit\n",
        graph_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session graph export: path="))
        .stdout(predicate::str::contains("format=mermaid"))
        .stdout(predicate::str::contains("nodes=2"))
        .stdout(predicate::str::contains("edges=1"));

    let raw_graph = fs::read_to_string(graph_path).expect("read graph");
    assert!(raw_graph.contains("graph TD"));
    assert!(raw_graph.contains("n1 --> n2"));
}

#[test]
fn regression_interactive_session_graph_export_command_invalid_destination_reports_error() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-graph-export-invalid.jsonl");
    let graph_dir = temp.path().join("graph-dir");
    fs::create_dir_all(&graph_dir).expect("mkdir");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin(format!(
        "/session-graph-export {}\n/help session-graph-export\n/quit\n",
        graph_dir.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "session graph export error: path=",
        ))
        .stdout(predicate::str::contains("is a directory"))
        .stdout(predicate::str::contains(
            "usage: /session-graph-export <path>",
        ));
}

#[test]
fn integration_interactive_branch_alias_command_set_use_and_list_flow() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-branch-alias.jsonl");
    let alias_path = session.with_extension("aliases.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"stable branch"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":3,
            "parent_id":1,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"hot branch"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/branch-alias set hotfix 2\n/branch 3\n/session\n/branch-alias use hotfix\n/session\n/branch-alias list\n/quit\n");

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("stdout should be utf8");
    assert!(stdout.contains("branch alias set: path="));
    assert!(stdout.contains("name=hotfix id=2"));
    assert!(stdout.contains("branch alias use: path="));
    assert!(stdout.contains("name=hotfix id=2"));
    assert!(stdout.contains("branch alias list: path="));
    assert!(stdout.contains("alias: name=hotfix id=2 status=ok"));

    let use_index = stdout.find("branch alias use: path=").expect("use output");
    let after_use = &stdout[use_index..];
    assert!(after_use.contains("active_head=2"));

    let alias_raw = fs::read_to_string(&alias_path).expect("read alias file");
    assert!(alias_raw.contains("\"schema_version\": 1"));
    assert!(alias_raw.contains("\"hotfix\": 2"));
}

#[test]
fn regression_interactive_branch_alias_command_stale_alias_reports_error_and_list_status() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-branch-alias-stale.jsonl");
    let alias_path = session.with_extension("aliases.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");
    let aliases = json!({
        "schema_version": 1,
        "aliases": {
            "legacy": 999
        }
    });
    fs::write(&alias_path, format!("{aliases}\n")).expect("write alias file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/branch-alias list\n/branch-alias use legacy\n/help branch-alias\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "alias: name=legacy id=999 status=stale",
        ))
        .stdout(predicate::str::contains(
            "alias points to unknown session id 999",
        ))
        .stdout(predicate::str::contains(
            "usage: /branch-alias <set|list|use> ...",
        ));
}

#[test]
fn regression_interactive_branch_alias_command_corrupt_file_reports_parse_error() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-branch-alias-corrupt.jsonl");
    let alias_path = session.with_extension("aliases.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");
    fs::write(&alias_path, "{invalid-json").expect("write malformed alias file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/branch-alias list\n/help branch-alias\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("branch alias error: path="))
        .stdout(predicate::str::contains("failed to parse alias file"))
        .stdout(predicate::str::contains(
            "usage: /branch-alias <set|list|use> ...",
        ));
}

#[test]
fn integration_interactive_session_bookmark_command_set_use_list_delete_flow() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-bookmark.jsonl");
    let bookmark_path = session.with_extension("bookmarks.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"stable branch"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":3,
            "parent_id":1,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"hot branch"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-bookmark set checkpoint 2\n/branch 3\n/session\n/session-bookmark use checkpoint\n/session\n/session-bookmark list\n/session-bookmark delete checkpoint\n/session-bookmark list\n/quit\n");

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("stdout should be utf8");
    assert!(stdout.contains("session bookmark set: path="));
    assert!(stdout.contains("name=checkpoint id=2"));
    assert!(stdout.contains("session bookmark use: path="));
    assert!(stdout.contains("name=checkpoint id=2"));
    assert!(stdout.contains("session bookmark list: path="));
    assert!(stdout.contains("bookmark: name=checkpoint id=2 status=ok"));
    assert!(stdout.contains("session bookmark delete: path="));
    assert!(stdout.contains("status=deleted"));

    let use_index = stdout
        .find("session bookmark use: path=")
        .expect("use output");
    let after_use = &stdout[use_index..];
    assert!(after_use.contains("active_head=2"));

    let bookmarks_raw = fs::read_to_string(&bookmark_path).expect("read bookmark file");
    assert!(bookmarks_raw.contains("\"schema_version\": 1"));
}

#[test]
fn regression_interactive_session_bookmark_command_stale_entry_reports_error() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-bookmark-stale.jsonl");
    let bookmark_path = session.with_extension("bookmarks.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");
    let bookmarks = json!({
        "schema_version": 1,
        "bookmarks": {
            "legacy": 999
        }
    });
    fs::write(&bookmark_path, format!("{bookmarks}\n")).expect("write bookmark file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin(
        "/session-bookmark list\n/session-bookmark use legacy\n/help session-bookmark\n/quit\n",
    );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "bookmark: name=legacy id=999 status=stale",
        ))
        .stdout(predicate::str::contains(
            "bookmark points to unknown session id 999",
        ))
        .stdout(predicate::str::contains(
            "usage: /session-bookmark <set|list|use|delete> ...",
        ));
}

#[test]
fn regression_interactive_session_bookmark_command_corrupt_file_reports_parse_error() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-bookmark-corrupt.jsonl");
    let bookmark_path = session.with_extension("bookmarks.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");
    fs::write(&bookmark_path, "{invalid-json").expect("write malformed bookmark file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 path"),
    ])
    .write_stdin("/session-bookmark list\n/help session-bookmark\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session bookmark error: path="))
        .stdout(predicate::str::contains(
            "failed to parse session bookmark file",
        ))
        .stdout(predicate::str::contains(
            "usage: /session-bookmark <set|list|use|delete> ...",
        ));
}

#[test]
fn integration_interactive_macro_command_lifecycle_flow() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-macro.jsonl");
    let macro_commands_file = temp.path().join("rewind.commands");
    let macro_store = temp.path().join(".tau").join("macros.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"leaf"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");
    fs::write(&macro_commands_file, "/branch 1\n/session\n").expect("write macro commands");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--session",
            session.to_str().expect("utf8 path"),
        ])
        .write_stdin(format!(
            "/macro save rewind {}\n/macro list\n/macro show rewind\n/macro run rewind --dry-run\n/macro run rewind\n/macro delete rewind\n/macro list\n/session\n/quit\n",
            macro_commands_file.display()
        ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("macro save: path="))
        .stdout(predicate::str::contains("name=rewind"))
        .stdout(predicate::str::contains("commands=2"))
        .stdout(predicate::str::contains("macro list: path="))
        .stdout(predicate::str::contains("macro: name=rewind commands=2"))
        .stdout(predicate::str::contains("macro show: path="))
        .stdout(predicate::str::contains("command: index=0 value=/branch 1"))
        .stdout(predicate::str::contains("command: index=1 value=/session"))
        .stdout(predicate::str::contains("mode=dry-run"))
        .stdout(predicate::str::contains("plan: command=/branch 1"))
        .stdout(predicate::str::contains("macro run: path="))
        .stdout(predicate::str::contains("mode=apply"))
        .stdout(predicate::str::contains("executed=2"))
        .stdout(predicate::str::contains("macro delete: path="))
        .stdout(predicate::str::contains("status=deleted"))
        .stdout(predicate::str::contains("remaining=0"))
        .stdout(predicate::str::contains("count=0"))
        .stdout(predicate::str::contains("macros: none"))
        .stdout(predicate::str::contains("active_head=1"));

    let macro_raw = fs::read_to_string(&macro_store).expect("read macro store");
    assert!(macro_raw.contains("\"schema_version\": 1"));
    assert!(!macro_raw.contains("\"rewind\""));
}

#[test]
fn regression_interactive_macro_command_invalid_name_and_missing_file_report_errors() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-macro-errors.jsonl");
    let missing_commands_file = temp.path().join("missing.commands");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--session",
            session.to_str().expect("utf8 path"),
        ])
        .write_stdin(format!(
            "/macro save 1bad {}\n/macro save quick {}\n/help macro\n/quit\n",
            missing_commands_file.display(),
            missing_commands_file.display()
        ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("macro error: path="))
        .stdout(predicate::str::contains(
            "macro name '1bad' must start with an ASCII letter",
        ))
        .stdout(predicate::str::contains("failed to read commands file"))
        .stdout(predicate::str::contains(
            "usage: /macro <save|run|list|show|delete> ...",
        ));
}

#[test]
fn regression_interactive_macro_command_reports_show_delete_usage_and_missing_macro() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-macro-usage.jsonl");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--session",
            session.to_str().expect("utf8 path"),
        ])
        .write_stdin("/macro show\n/macro delete\n/macro show missing\n/help macro\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("usage: /macro show <name>"))
        .stdout(predicate::str::contains("usage: /macro delete <name>"))
        .stdout(predicate::str::contains("unknown macro 'missing'"))
        .stdout(predicate::str::contains(
            "usage: /macro <save|run|list|show|delete> ...",
        ));
}

#[test]
fn regression_interactive_macro_command_corrupt_store_reports_parse_error() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session-macro-corrupt.jsonl");
    let macro_store_dir = temp.path().join(".tau");
    let macro_store_path = macro_store_dir.join("macros.json");
    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write session");
    fs::create_dir_all(&macro_store_dir).expect("mkdir macro dir");
    fs::write(&macro_store_path, "{invalid-json").expect("write malformed macro store");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--session",
            session.to_str().expect("utf8 path"),
        ])
        .write_stdin("/macro list\n/help macro\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("macro error: path="))
        .stdout(predicate::str::contains("failed to parse macro file"))
        .stdout(predicate::str::contains(
            "usage: /macro <save|run|list|show|delete> ...",
        ));
}

#[test]
fn integration_interactive_profile_command_full_lifecycle_roundtrip() {
    let temp = tempdir().expect("tempdir");
    let profile_store = temp.path().join(".tau").join("profiles.json");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--no-session",
        ])
        .write_stdin(
            "/profile save baseline\n/profile list\n/profile show baseline\n/profile load baseline\n/profile delete baseline\n/profile list\n/quit\n",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("profile save: path="))
        .stdout(predicate::str::contains("name=baseline"))
        .stdout(predicate::str::contains("status=saved"))
        .stdout(predicate::str::contains("profile list: path="))
        .stdout(predicate::str::contains("profiles=1"))
        .stdout(predicate::str::contains("profile: name=baseline"))
        .stdout(predicate::str::contains("profile show: path="))
        .stdout(predicate::str::contains("name=baseline status=found"))
        .stdout(predicate::str::contains("value: model=openai/gpt-4o-mini"))
        .stdout(predicate::str::contains("profile load: path="))
        .stdout(predicate::str::contains("status=in_sync"))
        .stdout(predicate::str::contains("diffs=0"))
        .stdout(predicate::str::contains("profile delete: path="))
        .stdout(predicate::str::contains("status=deleted"))
        .stdout(predicate::str::contains("remaining=0"))
        .stdout(predicate::str::contains("profiles=0"))
        .stdout(predicate::str::contains("names=none"));

    let raw = fs::read_to_string(profile_store).expect("read profile store");
    assert!(raw.contains("\"schema_version\": 1"));
    assert!(!raw.contains("\"baseline\""));
}

#[test]
fn regression_interactive_profile_command_invalid_name_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--no-session",
        ])
        .write_stdin("/profile save 1bad\n/help profile\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("profile error: path="))
        .stdout(predicate::str::contains(
            "profile name '1bad' must start with an ASCII letter",
        ))
        .stdout(predicate::str::contains(
            "usage: /profile <save|load|list|show|delete> ...",
        ));
}

#[test]
fn regression_interactive_profile_command_reports_show_list_delete_usage_errors() {
    let temp = tempdir().expect("tempdir");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--no-session",
        ])
        .write_stdin(
            "/profile show\n/profile list extra\n/profile delete missing\n/help profile\n/quit\n",
        );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("usage: /profile show <name>"))
        .stdout(predicate::str::contains("usage: /profile list"))
        .stdout(predicate::str::contains("unknown profile 'missing'"))
        .stdout(predicate::str::contains(
            "usage: /profile <save|load|list|show|delete> ...",
        ));
}

#[test]
fn regression_interactive_profile_command_invalid_schema_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let profile_dir = temp.path().join(".tau");
    let profile_store = profile_dir.join("profiles.json");
    fs::create_dir_all(&profile_dir).expect("mkdir profile dir");
    let invalid = json!({
        "schema_version": 99,
        "profiles": {
            "baseline": {
                "model": "openai/gpt-4o-mini",
                "fallback_models": [],
                "session": {
                    "enabled": false,
                    "path": null,
                    "import_mode": "merge"
                },
                "policy": {
                    "tool_policy_preset": "balanced",
                    "bash_profile": "balanced",
                    "bash_dry_run": false,
                    "os_sandbox_mode": "off",
                    "enforce_regular_files": true,
                    "bash_timeout_ms": 500,
                    "max_command_length": 4096,
                    "max_tool_output_bytes": 1024,
                    "max_file_read_bytes": 2048,
                    "max_file_write_bytes": 2048,
                    "allow_command_newlines": true
                }
            }
        }
    });
    fs::write(&profile_store, format!("{invalid}\n")).expect("write invalid profile store");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path())
        .args([
            "--model",
            "openai/gpt-4o-mini",
            "--openai-api-key",
            "test-openai-key",
            "--no-session",
        ])
        .write_stdin("/profile load baseline\n/help profile\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("profile error: path="))
        .stdout(predicate::str::contains(
            "unsupported profile schema_version 99",
        ))
        .stdout(predicate::str::contains(
            "usage: /profile <save|load|list|show|delete> ...",
        ));
}

#[test]
fn interactive_session_import_merge_remaps_collisions_by_default() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target.jsonl");
    let source = temp.path().join("source.jsonl");

    let target_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"target-root"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&target, format!("{target_raw}\n")).expect("write target");

    let source_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"import-root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"import-user"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&source, format!("{source_raw}\n")).expect("write source");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        target.to_str().expect("utf8 target path"),
    ])
    .write_stdin(format!("/session-import {}\n/quit\n", source.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session import complete"))
        .stdout(predicate::str::contains("mode=merge"))
        .stdout(predicate::str::contains("remapped_entries=2"))
        .stdout(predicate::str::contains("remapped_ids=1->2,2->3"));

    let entries = parse_session_entries(&target);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].id, 1);
    assert_eq!(entries[1].id, 2);
    assert_eq!(entries[1].parent_id, None);
    assert_eq!(entries[2].id, 3);
    assert_eq!(entries[2].parent_id, Some(2));
}

#[test]
fn integration_interactive_session_import_replace_mode_overwrites_target() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target-replace.jsonl");
    let source = temp.path().join("source-replace.jsonl");

    let target_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"target-root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"target-user"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&target, format!("{target_raw}\n")).expect("write target");

    let source_raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":10,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"import-root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":11,
            "parent_id":10,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"import-assistant"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&source, format!("{source_raw}\n")).expect("write source");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        target.to_str().expect("utf8 target path"),
        "--session-import-mode",
        "replace",
    ])
    .write_stdin(format!(
        "/session-import {}\n/session\n/quit\n",
        source.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session import complete"))
        .stdout(predicate::str::contains("mode=replace"))
        .stdout(predicate::str::contains("remapped_ids=none"))
        .stdout(predicate::str::contains("replaced_entries=2"));

    let entries = parse_session_entries(&target);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, 10);
    assert_eq!(entries[0].parent_id, None);
    assert_eq!(entries[1].id, 11);
    assert_eq!(entries[1].parent_id, Some(10));
}

#[test]
fn regression_session_repair_reports_removed_ids() {
    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("repair-session.jsonl");

    let raw = [
        json!({"record_type":"meta","schema_version":1}).to_string(),
        json!({
            "record_type":"entry",
            "id":1,
            "parent_id":null,
            "message":{
                "role":"system",
                "content":[{"type":"text","text":"root"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":2,
            "parent_id":99,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"invalid-parent"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":3,
            "parent_id":4,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"cycle-a"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":4,
            "parent_id":3,
            "message":{
                "role":"assistant",
                "content":[{"type":"text","text":"cycle-b"}],
                "is_error":false
            }
        })
        .to_string(),
        json!({
            "record_type":"entry",
            "id":5,
            "parent_id":1,
            "message":{
                "role":"user",
                "content":[{"type":"text","text":"healthy-head"}],
                "is_error":false
            }
        })
        .to_string(),
    ]
    .join("\n");
    fs::write(&session, format!("{raw}\n")).expect("write malformed session");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--session",
        session.to_str().expect("utf8 session path"),
    ])
    .write_stdin("/session-repair\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("repair complete"))
        .stdout(predicate::str::contains("invalid_parent_ids=2"))
        .stdout(predicate::str::contains("cycle_ids=3,4"));

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, 1);
    assert_eq!(entries[1].id, 5);
}

#[test]
fn openai_prompt_persists_session_and_supports_branch_from() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration openai response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session.jsonl");

    let mut first = binary_command();
    first.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "first prompt",
        "--session",
        session.to_str().expect("utf8 session path"),
    ]);

    first
        .assert()
        .success()
        .stdout(predicate::str::contains("integration openai response"));

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].message.role, "system");
    assert_eq!(entries[1].message.role, "user");
    assert_eq!(entries[2].message.role, "assistant");

    let mut second = binary_command();
    second.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "forked prompt",
        "--session",
        session.to_str().expect("utf8 session path"),
        "--branch-from",
        "2",
    ]);

    second.assert().success();

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[3].parent_id, Some(2));
    assert_eq!(entries[4].parent_id, Some(entries[3].id));

    openai.assert_calls(2);
}

#[test]
fn fallback_model_flag_routes_to_secondary_model_on_retryable_failure() {
    let server = MockServer::start();
    let primary = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("\"model\":\"gpt-primary\"")
            .header("x-tau-retry-attempt", "0");
        then.status(503).body("primary unavailable");
    });
    let fallback = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("\"model\":\"gpt-fallback\"")
            .header("x-tau-retry-attempt", "0");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "fallback route response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 2, "total_tokens": 8}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-primary",
        "--fallback-model",
        "openai/gpt-fallback",
        "--provider-max-retries",
        "0",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("fallback route response"));

    primary.assert_calls(1);
    fallback.assert_calls(1);
}

#[test]
fn integration_openrouter_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let openrouter = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration openrouter response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openrouter/openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("OPENROUTER_API_KEY", "test-openrouter-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration openrouter response"));

    openrouter.assert_calls(1);
}

#[test]
fn integration_groq_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let groq = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration groq response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "groq/llama-3.3-70b",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("GROQ_API_KEY", "test-groq-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration groq response"));

    groq.assert_calls(1);
}

#[test]
fn integration_xai_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let xai = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration xai response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "xai/grok-4",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("XAI_API_KEY", "test-xai-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration xai response"));

    xai.assert_calls(1);
}

#[test]
fn integration_mistral_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let mistral = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration mistral response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "mistral/mistral-large-latest",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("MISTRAL_API_KEY", "test-mistral-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration mistral response"));

    mistral.assert_calls(1);
}

#[test]
fn integration_azure_alias_uses_openai_client_with_api_key_header_and_api_version() {
    let server = MockServer::start();
    let azure = server.mock(|when, then| {
        when.method(POST)
            .path("/openai/deployments/test-deployment/chat/completions")
            .query_param("api-version", "2024-10-21")
            .header_exists("api-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration azure response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "azure/gpt-4o-mini",
        "--api-base",
        &format!("{}/openai/deployments/test-deployment", server.base_url()),
        "--azure-openai-api-version",
        "2024-10-21",
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("AZURE_OPENAI_API_KEY", "test-azure-key")
    .env_remove("OPENAI_API_KEY")
    .env_remove("TAU_API_KEY");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration azure response"));

    azure.assert_calls(1);
}

#[test]
fn anthropic_prompt_works_end_to_end() {
    let server = MockServer::start();
    let anthropic = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", "test-anthropic-key")
            .header("anthropic-version", "2023-06-01");
        then.status(200).json_body(json!({
            "content": [{"type": "text", "text": "integration anthropic response"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 8, "output_tokens": 3}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "anthropic/claude-sonnet-4-20250514",
        "--anthropic-api-base",
        &format!("{}/v1", server.base_url()),
        "--anthropic-api-key",
        "test-anthropic-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration anthropic response"));

    anthropic.assert_calls(1);
}

#[test]
fn google_prompt_works_end_to_end() {
    let server = MockServer::start();
    let google = server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:streamGenerateContent")
            .query_param("key", "test-google-key")
            .query_param("alt", "sse");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"integration \"}]}}]}\n\n",
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"google response\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":8,\"candidatesTokenCount\":3,\"totalTokenCount\":11}}\n\n"
            ));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "google/gemini-2.5-pro",
        "--google-api-base",
        &server.base_url(),
        "--google-api-key",
        "test-google-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration google response"));

    google.assert_calls(1);
}

#[test]
fn stream_output_flags_are_accepted_in_prompt_mode() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "streamed response"},
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
        "--prompt",
        "hello",
        "--no-session",
        "--stream-output",
        "false",
        "--stream-delay-ms",
        "0",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("streamed response"));

    openai.assert_calls(1);
}

#[test]
fn bash_profile_flags_are_accepted_in_prompt_mode() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "profile ok"},
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
        "--prompt",
        "hello",
        "--bash-profile",
        "strict",
        "--allow-command",
        "python,cargo-nextest*",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("profile ok"));

    openai.assert_calls(1);
}

#[test]
fn session_lock_flags_are_accepted_in_prompt_mode() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "lock flags ok"},
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
        "--prompt",
        "hello",
        "--session-lock-wait-ms",
        "250",
        "--session-lock-stale-ms",
        "0",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("lock flags ok"));

    openai.assert_calls(1);
}

#[test]
fn print_tool_policy_flag_outputs_effective_policy_json() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "policy output ok"},
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
        "--prompt",
        "hello",
        "--print-tool-policy",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\""))
        .stdout(predicate::str::contains("\"preset\""))
        .stdout(predicate::str::contains("\"max_file_write_bytes\""))
        .stdout(predicate::str::contains("\"os_sandbox_mode\""))
        .stdout(predicate::str::contains("policy output ok"));

    openai.assert_calls(1);
}

#[test]
fn tool_policy_preset_and_bash_dry_run_flags_are_accepted_in_prompt_mode() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "preset dry-run ok"},
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
        "--prompt",
        "hello",
        "--tool-policy-preset",
        "hardened",
        "--bash-dry-run",
        "--tool-policy-trace",
        "--print-tool-policy",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"preset\":\"hardened\""))
        .stdout(predicate::str::contains("\"bash_dry_run\":true"))
        .stdout(predicate::str::contains("\"tool_policy_trace\":true"))
        .stdout(predicate::str::contains("preset dry-run ok"));

    openai.assert_calls(1);
}

#[test]
fn package_validate_flag_reports_manifest_summary_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package validate:"))
        .stdout(predicate::str::contains("name=starter-bundle"))
        .stdout(predicate::str::contains("total_components=2"));
}

#[test]
fn extension_validate_flag_reports_manifest_summary_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant",
  "hooks": ["run-start", "run-end"],
  "permissions": ["read-files", "network"],
  "timeout_ms": 60000
}"#,
    )
    .expect("write extension manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension validate:"))
        .stdout(predicate::str::contains("id=issue-assistant"))
        .stdout(predicate::str::contains("permissions=2"))
        .stdout(predicate::str::contains("timeout_ms=60000"));
}

#[test]
fn regression_extension_validate_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write extension manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsupported extension manifest schema",
    ));
}

#[test]
fn extension_show_flag_reports_manifest_inventory_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant",
  "hooks": ["run-end", "run-start"],
  "permissions": ["network", "read-files"],
  "timeout_ms": 60000
}"#,
    )
    .expect("write extension manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-show",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension show:"))
        .stdout(predicate::str::contains("- hooks (2):"))
        .stdout(predicate::str::contains("- run-end"))
        .stdout(predicate::str::contains("- run-start"))
        .stdout(predicate::str::contains("- permissions (2):"))
        .stdout(predicate::str::contains("- network"))
        .stdout(predicate::str::contains("- read-files"));
}

#[test]
fn regression_extension_show_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write extension manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-show",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsupported extension manifest schema",
    ));
}

#[test]
fn extension_list_flag_reports_valid_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let valid_dir = root.join("issue-assistant");
    fs::create_dir_all(&valid_dir).expect("create valid extension dir");
    fs::write(
        valid_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write valid extension manifest");
    let invalid_dir = root.join("broken");
    fs::create_dir_all(&invalid_dir).expect("create invalid extension dir");
    fs::write(
        invalid_dir.join("extension.json"),
        r#"{
  "schema_version": 9,
  "id": "broken",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write invalid extension manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-list",
        "--extension-list-root",
        root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension list:"))
        .stdout(predicate::str::contains("count=1"))
        .stdout(predicate::str::contains("invalid=1"))
        .stdout(predicate::str::contains(
            "extension: id=issue-assistant version=0.1.0 runtime=process",
        ))
        .stdout(predicate::str::contains("invalid: manifest="))
        .stdout(predicate::str::contains(
            "unsupported extension manifest schema",
        ));
}

#[test]
fn regression_extension_list_flag_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("extensions.json");
    fs::write(&root_file, "{}").expect("write root file");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-list",
        "--extension-list-root",
        root_file.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("is not a directory"));
}

#[test]
fn extension_exec_flag_runs_process_hook_and_reports_success() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    let script_path = bin_dir.join("hook.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf '{\"ok\":true,\"result\":\"hook-processed\"}'\n",
    )
    .expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-exec-manifest",
        manifest_path.to_str().expect("utf8 path"),
        "--extension-exec-hook",
        "run-start",
        "--extension-exec-payload-file",
        payload_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension exec:"))
        .stdout(predicate::str::contains("hook=run-start"))
        .stdout(predicate::str::contains("extension exec response:"))
        .stdout(predicate::str::contains("\"ok\":true"));
}

#[test]
fn regression_extension_exec_flag_rejects_invalid_response() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    let script_path = bin_dir.join("bad.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf 'not-json'\n",
    )
    .expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/bad.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-exec-manifest",
        manifest_path.to_str().expect("utf8 path"),
        "--extension-exec-hook",
        "run-start",
        "--extension-exec-payload-file",
        payload_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("response must be valid JSON"));
}

#[test]
fn extension_runtime_hooks_wrap_prompt_with_run_start_and_run_end() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("issue-assistant");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let requests_path = extension_dir.join("requests.ndjson");
    let script_path = extension_dir.join("hook.sh");
    fs::write(
        &script_path,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\ninput=\"$(cat)\"\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"ok\":true}}'\n",
            requests_path.display()
        ),
    )
    .expect("write hook script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start", "run-end"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "runtime hooks ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello runtime hooks",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("runtime hooks ok"));

    let raw = fs::read_to_string(&requests_path).expect("read requests log");
    let rows = raw
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid json row"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["hook"], "run-start");
    assert_eq!(rows[1]["hook"], "run-end");
    assert_eq!(rows[0]["payload"]["schema_version"], 1);
    assert_eq!(rows[1]["payload"]["schema_version"], 1);
    assert_eq!(rows[0]["payload"]["hook"], "run-start");
    assert_eq!(rows[1]["payload"]["hook"], "run-end");
    assert!(rows[0]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert!(rows[1]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert_eq!(rows[0]["payload"]["data"]["prompt"], "hello runtime hooks");
    assert_eq!(rows[1]["payload"]["data"]["status"], "completed");

    openai.assert_calls(1);
}

#[test]
fn regression_extension_runtime_hook_timeout_does_not_fail_prompt() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("slow-extension");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("hook.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\nsleep 1\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "slow-extension",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start", "run-end"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "prompt still completed"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello runtime hooks",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("prompt still completed"))
        .stderr(predicate::str::contains("timed out"));

    openai.assert_calls(1);
}

#[test]
fn extension_message_transform_hook_rewrites_prompt_before_model_request() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\ncat >/dev/null\nprintf '{\"prompt\":\"transformed prompt text\"}'\n",
    )
    .expect("write transform script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }
    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .body_includes("transformed prompt text");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "transform ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "original prompt text",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("transform ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_extension_message_transform_invalid_response_falls_back_to_original_prompt() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("broken-transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\ncat >/dev/null\nprintf '{\"prompt\":123}'\n",
    )
    .expect("write transform script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }
    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "broken-transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .body_includes("original prompt text");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "fallback ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "original prompt text",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("fallback ok"))
        .stderr(predicate::str::contains("must be a string"));

    openai.assert_calls(1);
}

#[test]
fn regression_package_validate_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsupported package manifest schema",
    ));
}

#[test]
fn package_show_flag_reports_manifest_inventory_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args(["--package-show", manifest_path.to_str().expect("utf8 path")]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package show:"))
        .stdout(predicate::str::contains("templates (1):"))
        .stdout(predicate::str::contains("- review => templates/review.txt"))
        .stdout(predicate::str::contains("skills (1):"));
}

#[test]
fn regression_package_show_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "invalid",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args(["--package-show", manifest_path.to_str().expect("utf8 path")]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must follow x.y.z"));
}

#[test]
fn package_install_flag_installs_bundle_files_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"))
        .stdout(predicate::str::contains("total_components=1"));
    assert!(install_root
        .join("starter-bundle/1.0.0/templates/review.txt")
        .exists());
}

#[test]
fn package_install_flag_installs_remote_bundle_files_and_exits() {
    let server = MockServer::start();
    let remote_body = "remote template body";
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body(remote_body);
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(&package_root).expect("create package root");
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#,
            server.base_url()
        ),
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"))
        .stdout(predicate::str::contains("total_components=1"));
    assert_eq!(
        fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read installed template"),
        remote_body
    );
    remote_mock.assert();
}

#[test]
fn package_install_flag_accepts_valid_signed_manifest_when_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let signature = signing_key.sign(&fs::read(&manifest_path).expect("read manifest bytes"));
    fs::write(
        package_root.join("package.sig"),
        BASE64.encode(signature.to_bytes()),
    )
    .expect("write signature");
    let trust_root = format!(
        "publisher={}",
        BASE64.encode(signing_key.verifying_key().as_bytes())
    );

    let install_root = temp.path().join("installed");
    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"));
}

#[test]
fn package_install_flag_accepts_remote_signed_manifest_when_required() {
    let server = MockServer::start();
    let remote_body = "remote signed template";
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body(remote_body);
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(&package_root).expect("create package root");
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#,
            server.base_url()
        ),
    )
    .expect("write manifest");

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let signature = signing_key.sign(&fs::read(&manifest_path).expect("read manifest bytes"));
    fs::write(
        package_root.join("package.sig"),
        BASE64.encode(signature.to_bytes()),
    )
    .expect("write signature");
    let trust_root = format!(
        "publisher={}",
        BASE64.encode(signing_key.verifying_key().as_bytes())
    );

    let install_root = temp.path().join("installed");
    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"));
    remote_mock.assert();
}

#[test]
fn regression_package_install_flag_rejects_missing_component_source() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/missing.txt"}]
}"#,
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn regression_package_install_flag_rejects_remote_checksum_mismatch() {
    let server = MockServer::start();
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body("remote template");
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(&package_root).expect("create package root");
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{}"
  }}]
}}"#,
            server.base_url(),
            "0".repeat(64)
        ),
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("checksum mismatch"));
    remote_mock.assert();
}

#[test]
fn regression_package_install_flag_rejects_unsigned_when_signatures_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "must include signing_key and signature_file",
    ));
}

#[test]
fn package_update_flag_updates_existing_bundle_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    let template_path = package_root.join("templates/review.txt");
    fs::write(&template_path, "template body v1").expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    fs::write(&template_path, "template body v2").expect("update template source");
    let mut update_cmd = binary_command();
    update_cmd.args([
        "--package-update",
        manifest_path.to_str().expect("utf8 path"),
        "--package-update-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    update_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package update:"))
        .stdout(predicate::str::contains("updated=1"))
        .stdout(predicate::str::contains("name=starter-bundle"));
    assert_eq!(
        fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read updated template"),
        "template body v2"
    );
}

#[test]
fn package_update_flag_accepts_signed_manifest_when_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    let template_path = package_root.join("templates/review.txt");
    fs::write(&template_path, "template body v1").expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let write_signature = || {
        let signature = signing_key.sign(&fs::read(&manifest_path).expect("read manifest bytes"));
        fs::write(
            package_root.join("package.sig"),
            BASE64.encode(signature.to_bytes()),
        )
        .expect("write signature");
    };
    write_signature();
    let trust_root = format!(
        "publisher={}",
        BASE64.encode(signing_key.verifying_key().as_bytes())
    );

    let install_root = temp.path().join("installed");
    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);
    install_cmd.assert().success();

    fs::write(&template_path, "template body v2").expect("update template source");
    write_signature();
    let mut update_cmd = binary_command();
    update_cmd.args([
        "--package-update",
        manifest_path.to_str().expect("utf8 path"),
        "--package-update-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);

    update_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package update:"))
        .stdout(predicate::str::contains("name=starter-bundle"));
}

#[test]
fn regression_package_update_flag_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-update",
        manifest_path.to_str().expect("utf8 path"),
        "--package-update-root",
        temp.path().join("installed").to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("is not installed"));
}

#[test]
fn package_conflicts_flag_reports_conflicts_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");

    let install_package = |name: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(source_root.join("templates/review.txt"), body).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha", "alpha body");
    install_package("zeta", "zeta body");

    let invalid_dir = install_root.join("broken/9.9.9");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");
    fs::write(
        invalid_dir.join("package.json"),
        r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "9.9.9",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write invalid manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-conflicts",
        "--package-conflicts-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package conflicts:"))
        .stdout(predicate::str::contains("conflicts=1"))
        .stdout(predicate::str::contains("invalid=1"))
        .stdout(predicate::str::contains("conflict: kind=templates"))
        .stdout(predicate::str::contains("package invalid:"));
}

#[test]
fn regression_package_conflicts_flag_reports_none_when_no_conflicts_exist() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");

    let install_package = |name: &str, path: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        let component_dir = source_root.join(
            std::path::Path::new(path)
                .parent()
                .expect("component parent"),
        );
        fs::create_dir_all(&component_dir).expect("create component dir");
        fs::write(source_root.join(path), format!("{name} body")).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"{path}"}}]
}}"#
            ),
        )
        .expect("write manifest");
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha", "templates/review-a.txt");
    install_package("zeta", "templates/review-z.txt");

    let mut cmd = binary_command();
    cmd.args([
        "--package-conflicts",
        "--package-conflicts-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package conflicts:"))
        .stdout(predicate::str::contains("conflicts=0"))
        .stdout(predicate::str::contains("conflicts: none"));
}

#[test]
fn package_activate_flag_materializes_components_and_exits() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let destination_root = temp.path().join("activated");
    let source_root = temp.path().join("bundle");
    fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
    fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    fs::write(source_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    fs::write(source_root.join("skills/checks/SKILL.md"), "# checks").expect("write skill source");
    let manifest_path = source_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let mut activate_cmd = binary_command();
    activate_cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        destination_root.to_str().expect("utf8 path"),
    ]);

    activate_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package activate:"))
        .stdout(predicate::str::contains("policy=error"))
        .stdout(predicate::str::contains("activated_components=2"));
    assert_eq!(
        fs::read_to_string(destination_root.join("templates/review.txt"))
            .expect("read activated template"),
        "template body"
    );
    assert_eq!(
        fs::read_to_string(destination_root.join("skills/checks/SKILL.md"))
            .expect("read activated skill"),
        "# checks"
    );
    assert_eq!(
        fs::read_to_string(destination_root.join("skills/checks.md"))
            .expect("read activated skill alias"),
        "# checks"
    );
}

#[test]
fn integration_package_activate_on_startup_loads_activated_skill_for_prompt() {
    let temp = tempdir().expect("tempdir");
    let source_root = temp.path().join("bundle");
    fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    fs::write(
        source_root.join("skills/checks/SKILL.md"),
        "Activated checks body",
    )
    .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cmd = binary_command();
    install_cmd.current_dir(temp.path()).args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        ".tau/packages",
    ]);
    install_cmd.assert().success();

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: checks\nActivated checks body"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok startup activation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 2, "total_tokens": 10}
        }));
    });

    let mut cmd = binary_command();
    cmd.current_dir(temp.path()).args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--package-activate-on-startup",
        "--package-activate-root",
        ".tau/packages",
        "--package-activate-destination",
        ".tau/packages-active",
        "--skill",
        "checks",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package activate:"))
        .stdout(predicate::str::contains("ok startup activation"));
    openai.assert_calls(1);
    assert_eq!(
        fs::read_to_string(temp.path().join(".tau/packages-active/skills/checks.md"))
            .expect("read activated alias"),
        "Activated checks body"
    );
}

#[test]
fn package_activate_flag_keep_last_policy_resolves_conflicts() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let destination_root = temp.path().join("activated");
    let install_package = |name: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(source_root.join("templates/review.txt"), body).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha", "alpha body");
    install_package("zeta", "zeta body");

    let mut activate_cmd = binary_command();
    activate_cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        destination_root.to_str().expect("utf8 path"),
        "--package-activate-conflict-policy",
        "keep-last",
    ]);

    activate_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package activate:"))
        .stdout(predicate::str::contains("policy=keep-last"))
        .stdout(predicate::str::contains("conflicts_detected=1"));
    assert_eq!(
        fs::read_to_string(destination_root.join("templates/review.txt"))
            .expect("read activated template"),
        "zeta body"
    );
}

#[test]
fn regression_package_activate_flag_error_policy_rejects_conflicts() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_package = |name: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(
            source_root.join("templates/review.txt"),
            format!("{name} body"),
        )
        .expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha");
    install_package("zeta");

    let mut cmd = binary_command();
    cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        temp.path().join("activated").to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("package activation conflict"));
}

#[test]
fn regression_package_activate_flag_rejects_invalid_installed_manifest_entries() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
    fs::write(source_root.join("templates/review.txt"), "valid body").expect("write template");
    let manifest_path = source_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "valid-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let invalid_dir = install_root.join("broken/9.9.9");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");
    fs::write(
        invalid_dir.join("package.json"),
        r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "9.9.9",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write invalid manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        temp.path().join("activated").to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "invalid installed package entries",
    ));
}

#[test]
fn package_list_flag_reports_installed_packages_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let mut list_cmd = binary_command();
    list_cmd.args([
        "--package-list",
        "--package-list-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    list_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package list:"))
        .stdout(predicate::str::contains("packages=1"))
        .stdout(predicate::str::contains("invalid=0"))
        .stdout(predicate::str::contains(
            "package: name=starter-bundle version=1.0.0",
        ));
}

#[test]
fn regression_package_list_flag_reports_invalid_manifest_entries() {
    let temp = tempdir().expect("tempdir");
    let list_root = temp.path().join("installed");
    let invalid_dir = list_root.join("broken/9.9.9");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");
    fs::write(
        invalid_dir.join("package.json"),
        r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "9.9.9",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write invalid manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-list",
        "--package-list-root",
        list_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package list:"))
        .stdout(predicate::str::contains("packages=0"))
        .stdout(predicate::str::contains("invalid=1"))
        .stdout(predicate::str::contains("package invalid:"));
}

#[test]
fn package_remove_flag_removes_installed_bundle_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let mut remove_cmd = binary_command();
    remove_cmd.args([
        "--package-remove",
        "starter-bundle@1.0.0",
        "--package-remove-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    remove_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package remove:"))
        .stdout(predicate::str::contains("status=removed"));
    assert!(!install_root.join("starter-bundle/1.0.0").exists());
}

#[test]
fn regression_package_remove_flag_rejects_invalid_coordinate() {
    let temp = tempdir().expect("tempdir");
    let mut cmd = binary_command();
    cmd.args([
        "--package-remove",
        "starter-bundle",
        "--package-remove-root",
        temp.path().to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must follow <name>@<version>"));
}

#[test]
fn package_rollback_flag_keeps_target_and_removes_other_versions() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_version = |version: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{version}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(source_root.join("templates/review.txt"), body).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "{version}",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");

        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };

    install_version("1.0.0", "v1");
    install_version("2.0.0", "v2");

    let mut rollback_cmd = binary_command();
    rollback_cmd.args([
        "--package-rollback",
        "starter-bundle@1.0.0",
        "--package-rollback-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    rollback_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package rollback:"))
        .stdout(predicate::str::contains("status=rolled_back"))
        .stdout(predicate::str::contains("removed_versions=1"));
    assert!(install_root.join("starter-bundle/1.0.0").exists());
    assert!(!install_root.join("starter-bundle/2.0.0").exists());
}

#[test]
fn regression_package_rollback_flag_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let mut cmd = binary_command();
    cmd.args([
        "--package-rollback",
        "starter-bundle@1.0.0",
        "--package-rollback-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("is not installed"));
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

#[test]
fn turn_timeout_flag_times_out_prompt_and_keeps_process_healthy() {
    let server = MockServer::start();
    let _openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200)
            .delay(std::time::Duration::from_millis(150))
            .json_body(json!({
                "choices": [{
                    "message": {"content": "slow response"},
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
        "--prompt",
        "hello",
        "--turn-timeout-ms",
        "20",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("request timed out"));
}

#[test]
fn selected_skill_is_included_in_system_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: focus\nAlways use checklist"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 1, "total_tokens": 7}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("create skills dir");
    fs::write(skills_dir.join("focus.md"), "Always use checklist").expect("write skill file");

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
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill",
        "focus",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("ok"));
    openai.assert_calls(1);
}

#[test]
fn install_skill_flag_installs_skill_before_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: installable\nInstalled skill body"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok install"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 1, "total_tokens": 7}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let source_skill = temp.path().join("installable.md");
    fs::write(&source_skill, "Installed skill body").expect("write source skill");

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
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill",
        source_skill.to_str().expect("utf8 path"),
        "--skill",
        "installable",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills install: installed=1"))
        .stdout(predicate::str::contains("ok install"));
    assert!(skills_dir.join("installable.md").exists());
    openai.assert_calls(1);
}

#[test]
fn skills_lock_write_flag_generates_lockfile_for_local_install() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok lock"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 1, "total_tokens": 5}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let source_skill = temp.path().join("installable.md");
    fs::write(&source_skill, "Installed skill body").expect("write source skill");

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
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill",
        source_skill.to_str().expect("utf8 path"),
        "--skills-lock-write",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write: path="))
        .stdout(predicate::str::contains("ok lock"));

    let lock_path = skills_dir.join("skills.lock.json");
    assert!(lock_path.exists());
    let raw = fs::read_to_string(&lock_path).expect("read lockfile");
    let lock: serde_json::Value = serde_json::from_str(&raw).expect("parse lockfile");
    assert_eq!(lock["schema_version"], 1);
    assert_eq!(lock["entries"][0]["file"], "installable.md");
    assert_eq!(lock["entries"][0]["source"]["kind"], "local");
    openai.assert_calls(1);
}

#[test]
fn skills_sync_flag_succeeds_for_matching_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-sync",
        "--no-session",
    ])
    .write_stdin("/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: in-sync"));
}

#[test]
fn regression_skills_sync_flag_fails_on_drift() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": "deadbeef",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-sync",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("skills sync drift detected"));
}

#[test]
fn interactive_skills_list_command_prints_inventory() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("zeta.md"), "zeta body").expect("write zeta");
    fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-list\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills list: path="))
        .stdout(predicate::str::contains("count=2"))
        .stdout(predicate::str::contains("skill: name=alpha file=alpha.md"))
        .stdout(predicate::str::contains("skill: name=zeta file=zeta.md"));
}

#[test]
fn regression_interactive_skills_list_command_with_args_prints_usage_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-list extra\n/help skills-list\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("usage: /skills-list"))
        .stdout(predicate::str::contains("command: /skills-list"));
}

#[test]
fn interactive_skills_show_command_displays_skill_metadata_and_content() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-show checklist\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills show: path="))
        .stdout(predicate::str::contains("name=checklist"))
        .stdout(predicate::str::contains("file=checklist.md"))
        .stdout(predicate::str::contains("Always run tests"));
}

#[test]
fn regression_interactive_skills_show_command_reports_unknown_skill_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("known.md"), "known body").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-show missing\n/help skills-show\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills show error: path="))
        .stdout(predicate::str::contains("unknown skill 'missing'"))
        .stdout(predicate::str::contains("usage: /skills-show <name>"));
}

#[test]
fn interactive_skills_search_command_ranks_name_hits_before_content_hits() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write checklist");
    fs::write(skills_dir.join("quality.md"), "Use checklist for review").expect("write quality");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-search checklist\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills search: path="))
        .stdout(predicate::str::contains("matched=2"))
        .stdout(predicate::str::contains(
            "skill: name=checklist file=checklist.md match=name",
        ))
        .stdout(predicate::str::contains(
            "skill: name=quality file=quality.md match=content",
        ));
}

#[test]
fn regression_interactive_skills_search_command_invalid_limit_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-search checklist 0\n/help skills-search\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills search error: path="))
        .stdout(predicate::str::contains(
            "max_results must be greater than zero",
        ))
        .stdout(predicate::str::contains(
            "usage: /skills-search <query> [max_results]",
        ));
}

#[test]
fn interactive_skills_lock_diff_command_reports_in_sync_state() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-lock-diff\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock diff: in-sync"))
        .stdout(predicate::str::contains("expected_entries=1"))
        .stdout(predicate::str::contains("actual_entries=1"));
}

#[test]
fn integration_interactive_skills_lock_diff_command_supports_json_output() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let lock_path = temp.path().join("custom.lock.json");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-lock-diff {} --json\n/quit\n",
        lock_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"in_sync\""))
        .stdout(predicate::str::contains("\"in_sync\":true"));
}

#[test]
fn regression_interactive_skills_lock_diff_command_invalid_args_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-lock-diff one two\n/help skills-lock-diff\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock diff error: path="))
        .stdout(predicate::str::contains(
            "usage: /skills-lock-diff [lockfile_path] [--json]",
        ));
}

#[test]
fn interactive_skills_prune_command_dry_run_lists_candidates_without_deleting() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("tracked.md"), "tracked body").expect("write tracked");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let tracked_sha = format!("{:x}", Sha256::digest("tracked body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "tracked",
            "file": "tracked.md",
            "sha256": tracked_sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-prune\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune: mode=dry-run"))
        .stdout(predicate::str::contains(
            "prune: file=stale.md action=would_delete",
        ));

    assert!(skills_dir.join("stale.md").exists());
}

#[test]
fn integration_interactive_skills_prune_command_apply_deletes_untracked_files() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("tracked.md"), "tracked body").expect("write tracked");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let tracked_sha = format!("{:x}", Sha256::digest("tracked body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "tracked",
            "file": "tracked.md",
            "sha256": tracked_sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-prune --apply\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune: mode=apply"))
        .stdout(predicate::str::contains(
            "prune: file=stale.md action=delete",
        ))
        .stdout(predicate::str::contains(
            "prune: file=stale.md status=deleted",
        ))
        .stdout(predicate::str::contains(
            "skills prune result: mode=apply deleted=1 failed=0",
        ));

    assert!(skills_dir.join("tracked.md").exists());
    assert!(!skills_dir.join("stale.md").exists());
}

#[test]
fn regression_interactive_skills_prune_command_missing_lockfile_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let missing_lock = temp.path().join("missing.lock.json");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-prune {} --apply\n/help skills-prune\n/quit\n",
        missing_lock.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune error: path="))
        .stdout(predicate::str::contains("failed to read skills lockfile"))
        .stdout(predicate::str::contains(
            "usage: /skills-prune [lockfile_path] [--dry-run|--apply]",
        ));
}

#[test]
fn regression_interactive_skills_prune_command_rejects_unsafe_lockfile_entry() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "escape",
            "file": "../escape.md",
            "sha256": "abc123",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-prune\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune error: path="))
        .stdout(predicate::str::contains(
            "unsafe lockfile entry '../escape.md'",
        ));
}

#[test]
fn integration_interactive_skills_trust_list_command_reports_mixed_statuses() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    let payload = json!({
        "roots": [
            {
                "id": "zeta",
                "public_key": "eg==",
                "revoked": false,
                "expires_unix": 1,
                "rotated_from": null
            },
            {
                "id": "alpha",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            },
            {
                "id": "beta",
                "public_key": "Yg==",
                "revoked": true,
                "expires_unix": null,
                "rotated_from": "alpha"
            }
        ]
    });
    fs::write(&trust_path, format!("{payload}\n")).expect("write trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-trust-list\n/quit\n");

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("stdout should be utf8");
    assert!(stdout.contains("skills trust list: path="));
    assert!(stdout.contains("count=3"));
    let alpha_index = stdout
        .find("root: id=alpha revoked=false")
        .expect("alpha row");
    let beta_index = stdout.find("root: id=beta revoked=true").expect("beta row");
    let zeta_index = stdout
        .find("root: id=zeta revoked=false")
        .expect("zeta row");
    assert!(alpha_index < beta_index);
    assert!(beta_index < zeta_index);
    assert!(stdout.contains("rotated_from=alpha status=revoked"));
    assert!(stdout.contains("expires_unix=1 rotated_from=none status=expired"));
}

#[test]
fn regression_interactive_skills_trust_list_command_malformed_json_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    fs::write(&trust_path, "{invalid-json").expect("write malformed trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-trust-list {}\n/help skills-trust-list\n/quit\n",
        trust_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills trust list error: path="))
        .stdout(predicate::str::contains(
            "failed to parse trusted root file",
        ))
        .stdout(predicate::str::contains(
            "usage: /skills-trust-list [trust_root_file]",
        ));
}

#[test]
fn integration_interactive_skills_trust_mutation_commands_roundtrip() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    let payload = json!({
        "roots": [
            {
                "id": "old",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            }
        ]
    });
    fs::write(&trust_path, format!("{payload}\n")).expect("write trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(
        "/skills-trust-add extra=Yg==\n/skills-trust-revoke extra\n/skills-trust-rotate old:new=Yw==\n/skills-trust-list\n/quit\n",
    );

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("stdout should be utf8");
    assert!(stdout.contains("skills trust add: path="));
    assert!(stdout.contains("id=extra"));
    assert!(stdout.contains("skills trust revoke: path="));
    assert!(stdout.contains("id=extra"));
    assert!(stdout.contains("skills trust rotate: path="));
    assert!(stdout.contains("old_id=old new_id=new"));
    assert!(stdout.contains("root: id=old"));
    assert!(stdout.contains("root: id=new"));
    assert!(stdout.contains("rotated_from=old status=active"));
    assert!(stdout.contains("root: id=extra"));
    assert!(stdout.contains("status=revoked"));

    let trust_raw = fs::read_to_string(&trust_path).expect("read trust file");
    assert!(trust_raw.contains("\"id\": \"old\""));
    assert!(trust_raw.contains("\"revoked\": true"));
    assert!(trust_raw.contains("\"id\": \"new\""));
    assert!(trust_raw.contains("\"rotated_from\": \"old\""));
}

#[test]
fn regression_interactive_skills_trust_add_without_configured_path_reports_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-trust-add root=YQ==\n/help skills-trust-add\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "skills trust add error: path=none",
        ))
        .stdout(predicate::str::contains(
            "usage: /skills-trust-add <id=base64_key> [trust_root_file]",
        ));
}

#[test]
fn regression_interactive_skills_trust_revoke_unknown_id_reports_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    fs::write(&trust_path, "[]\n").expect("write trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-trust-revoke missing\n/quit\n");

    cmd.assert().success().stdout(predicate::str::contains(
        "cannot revoke unknown trust key id 'missing'",
    ));
}

#[test]
fn integration_interactive_skills_verify_command_reports_combined_compliance() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    fs::write(skills_dir.join("extra.md"), "untracked body").expect("write extra");

    let lock_path = skills_dir.join("skills.lock.json");
    let trust_path = temp.path().join("trust-roots.json");
    let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let signature = "c2ln";
    let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": skill_sha,
            "source": {
                "kind": "remote",
                "url": "https://example.com/focus.md",
                "expected_sha256": skill_sha,
                "signing_key_id": "unknown",
                "signature": signature,
                "signer_public_key": "YQ==",
                "signature_sha256": signature_sha
            }
        }]
    });
    fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
    let trust = json!({
        "roots": [{
            "id": "root",
            "public_key": "YQ==",
            "revoked": false,
            "expires_unix": null,
            "rotated_from": null
        }]
    });
    fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-verify\n/skills-verify --json\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills verify: status=fail"))
        .stdout(predicate::str::contains(
            "sync: expected_entries=1 actual_entries=2",
        ))
        .stdout(predicate::str::contains("signature=untrusted key=unknown"))
        .stdout(predicate::str::contains("\"status\":\"fail\""));
}

#[test]
fn regression_interactive_skills_verify_command_invalid_args_report_usage() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-verify one two three\n/help skills-verify\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills verify error: path="))
        .stdout(predicate::str::contains(
            "usage: /skills-verify [lockfile_path] [trust_root_file] [--json]",
        ));
}

#[test]
fn interactive_skills_lock_write_command_writes_default_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-lock-write\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write: path="))
        .stdout(predicate::str::contains("entries=1"));

    let lock_path = skills_dir.join("skills.lock.json");
    let raw = fs::read_to_string(lock_path).expect("read lock");
    assert!(raw.contains("\"file\": \"focus.md\""));
}

#[test]
fn integration_interactive_skills_lock_write_command_accepts_optional_lockfile_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let custom_lock_path = temp.path().join("custom.lock.json");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-lock-write {}\n/quit\n",
        custom_lock_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write: path="))
        .stdout(predicate::str::contains(
            custom_lock_path.display().to_string(),
        ));

    let raw = fs::read_to_string(custom_lock_path).expect("read custom lock");
    assert!(raw.contains("\"file\": \"focus.md\""));
}

#[test]
fn regression_interactive_skills_lock_write_command_reports_error_and_continues_loop() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let blocking_path = temp.path().join("blocking.lock");
    fs::create_dir_all(&blocking_path).expect("create blocking path");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-lock-write {}\n/help skills-lock-write\n/quit\n",
        blocking_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write error: path="))
        .stdout(predicate::str::contains(
            "usage: /skills-lock-write [lockfile_path]",
        ));
}

#[test]
fn interactive_skills_sync_command_uses_default_lockfile_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-sync\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: in-sync"))
        .stdout(predicate::str::contains("expected_entries=1"))
        .stdout(predicate::str::contains("actual_entries=1"));
}

#[test]
fn integration_interactive_skills_sync_command_accepts_optional_lockfile_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let lock_path = temp.path().join("custom.lock.json");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!("/skills-sync {}\n/quit\n", lock_path.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: in-sync"))
        .stdout(predicate::str::contains(lock_path.display().to_string()));
}

#[test]
fn regression_interactive_skills_sync_command_reports_drift_and_continues_loop() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": "deadbeef",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-sync\n/help skills-sync\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: drift"))
        .stdout(predicate::str::contains("changed=focus.md"))
        .stdout(predicate::str::contains(
            "usage: /skills-sync [lockfile_path]",
        ));
}

#[test]
fn install_skill_url_with_sha256_verification_works_end_to_end() {
    let server = MockServer::start();
    let remote_body = "Remote checksum skill";
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));

    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/remote.md");
        then.status(200).body(remote_body);
    });

    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: remote\nRemote checksum skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok remote"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 1, "total_tokens": 8}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

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
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill-url",
        &format!("{}/skills/remote.md", server.base_url()),
        "--install-skill-sha256",
        &checksum,
        "--skill",
        "remote",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "remote skills install: installed=1",
        ))
        .stdout(predicate::str::contains("ok remote"));
    assert!(skills_dir.join("remote.md").exists());
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn integration_install_skill_url_offline_replay_uses_cache_without_network() {
    let server = MockServer::start();
    let remote_body = "Remote cached skill";
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));

    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/cached.md");
        then.status(200).body(remote_body);
    });

    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok remote cache"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 1, "total_tokens": 8}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let cache_dir = temp.path().join("skills-cache");

    let mut warm = binary_command();
    warm.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--install-skill-url",
        &format!("{}/skills/cached.md", server.base_url()),
        "--install-skill-sha256",
        &checksum,
        "--skill",
        "cached",
        "--no-session",
    ]);
    warm.assert().success();

    let mut replay = binary_command();
    replay.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--install-skill-url",
        &format!("{}/skills/cached.md", server.base_url()),
        "--install-skill-sha256",
        &checksum,
        "--skill",
        "cached",
        "--no-session",
    ]);
    replay
        .assert()
        .success()
        .stdout(predicate::str::contains("remote skills install:"));

    remote.assert_calls(1);
    openai.assert_calls(2);
}

#[test]
fn regression_skills_offline_mode_without_warm_remote_cache_fails() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--install-skill-url",
        "https://example.com/skills/missing.md",
        "--install-skill-sha256",
        "deadbeef",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("offline cache miss for skill URL"));
}

#[test]
fn install_skill_from_registry_works_end_to_end() {
    let server = MockServer::start();
    let skill_body = "Registry-driven skill";
    let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "reg",
            "url": format!("{}/skills/reg.md", server.base_url()),
            "sha256": skill_sha
        }]
    })
    .to_string();
    let registry_sha = format!("{:x}", Sha256::digest(registry_body.as_bytes()));

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });
    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/reg.md");
        then.status(200).body(skill_body);
    });
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: reg\nRegistry-driven skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok registry"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

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
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--install-skill-from-registry",
        "reg",
        "--skill",
        "reg",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "registry skills install: installed=1",
        ))
        .stdout(predicate::str::contains("ok registry"));
    assert!(skills_dir.join("reg.md").exists());
    registry.assert_calls(1);
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn integration_install_skill_from_registry_offline_replay_uses_cache_without_network() {
    let server = MockServer::start();
    let skill_body = "Registry cached skill";
    let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "reg-cache",
            "url": format!("{}/skills/reg-cache.md", server.base_url()),
            "sha256": skill_sha
        }]
    })
    .to_string();
    let registry_sha = format!("{:x}", Sha256::digest(registry_body.as_bytes()));

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry-cache.json");
        then.status(200).body(registry_body);
    });
    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/reg-cache.md");
        then.status(200).body(skill_body);
    });
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok registry cache"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let cache_dir = temp.path().join("skills-cache");

    let mut warm = binary_command();
    warm.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry-cache.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--install-skill-from-registry",
        "reg-cache",
        "--skill",
        "reg-cache",
        "--no-session",
    ]);
    warm.assert().success();

    let mut replay = binary_command();
    replay.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--skill-registry-url",
        &format!("{}/registry-cache.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--install-skill-from-registry",
        "reg-cache",
        "--skill",
        "reg-cache",
        "--no-session",
    ]);
    replay
        .assert()
        .success()
        .stdout(predicate::str::contains("registry skills install:"));

    registry.assert_calls(1);
    remote.assert_calls(1);
    openai.assert_calls(2);
}

#[test]
fn regression_skills_offline_mode_without_warm_registry_cache_fails() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--skill-registry-url",
        "https://example.com/registry.json",
        "--install-skill-from-registry",
        "review",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("offline cache miss for registry"));
}

#[test]
fn install_signed_skill_from_registry_with_trust_root_works_end_to_end() {
    let server = MockServer::start();
    let root = SigningKey::from_bytes(&[41u8; 32]);
    let publisher = SigningKey::from_bytes(&[42u8; 32]);
    let root_public_key = BASE64.encode(root.verifying_key().to_bytes());
    let publisher_public_key = BASE64.encode(publisher.verifying_key().to_bytes());
    let publisher_certificate = BASE64.encode(
        root.sign(format!("tau-skill-key-v1:publisher:{publisher_public_key}").as_bytes())
            .to_bytes(),
    );

    let skill_body = "Signed registry skill";
    let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
    let skill_signature = BASE64.encode(publisher.sign(skill_body.as_bytes()).to_bytes());
    let expected_signature_sha = format!("{:x}", Sha256::digest(skill_signature.trim().as_bytes()));
    let registry_body = json!({
        "version": 1,
        "keys": [{
            "id":"publisher",
            "public_key": publisher_public_key,
            "signed_by":"root",
            "signature": publisher_certificate
        }],
        "skills": [{
            "name": "reg-secure",
            "url": format!("{}/skills/reg-secure.md", server.base_url()),
            "sha256": skill_sha,
            "signing_key":"publisher",
            "signature": skill_signature
        }]
    })
    .to_string();
    let registry_sha = format!("{:x}", Sha256::digest(registry_body.as_bytes()));

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });
    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/reg-secure.md");
        then.status(200).body(skill_body);
    });
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: reg-secure\nSigned registry skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok signed registry"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

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
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--skill-trust-root",
        &format!("root={root_public_key}"),
        "--require-signed-skills",
        "--install-skill-from-registry",
        "reg-secure",
        "--skills-lock-write",
        "--skill",
        "reg-secure",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "registry skills install: installed=1",
        ))
        .stdout(predicate::str::contains("skills lock write: path="))
        .stdout(predicate::str::contains("ok signed registry"));
    assert!(skills_dir.join("reg-secure.md").exists());
    let lock_path = skills_dir.join("skills.lock.json");
    let raw = fs::read_to_string(lock_path).expect("read lockfile");
    let lock: serde_json::Value = serde_json::from_str(&raw).expect("parse lockfile");
    assert_eq!(lock["entries"][0]["source"]["kind"], "registry");
    assert_eq!(lock["entries"][0]["source"]["signing_key_id"], "publisher");
    assert_eq!(
        lock["entries"][0]["source"]["signature_sha256"],
        expected_signature_sha
    );
    registry.assert_calls(1);
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn require_signed_skills_rejects_unsigned_registry_entries() {
    let server = MockServer::start();
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "unsigned",
            "url": format!("{}/skills/unsigned.md", server.base_url())
        }]
    })
    .to_string();

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

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
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--require-signed-skills",
        "--install-skill-from-registry",
        "unsigned",
        "--no-session",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsigned but signatures are required",
    ));
    registry.assert_calls(1);
}
