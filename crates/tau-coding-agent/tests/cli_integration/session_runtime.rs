//! CLI integration tests for session runtime persistence, restore, and help behavior.

use super::*;

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
        .stdout(predicate::str::contains("doctor summary: checks=18"))
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
            "doctor check: key=browser_automation.npx",
        ))
        .stdout(predicate::str::contains(
            "doctor check: key=browser_automation.playwright_cli",
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
