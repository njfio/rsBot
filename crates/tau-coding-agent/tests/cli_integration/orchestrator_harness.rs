//! Fixture-driven CLI integration coverage for plan-first orchestration flows.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use httpmock::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use tempfile::tempdir;

use super::*;

const ORCHESTRATOR_FIXTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct OrchestratorFixture {
    schema_version: u32,
    name: String,
    prompt: String,
    planner_response: String,
    delegated_step_responses: Vec<String>,
    consolidation_response: String,
    expected_route_roles: Vec<String>,
}

fn orchestrator_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("orchestrator-harness")
        .join(name)
}

fn load_orchestrator_fixture(name: &str) -> OrchestratorFixture {
    let path = orchestrator_fixture_path(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let fixture = serde_json::from_str::<OrchestratorFixture>(&raw)
        .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", path.display()));
    assert_eq!(
        fixture.schema_version,
        ORCHESTRATOR_FIXTURE_SCHEMA_VERSION,
        "unsupported fixture schema_version in {}",
        path.display()
    );
    fixture
}

fn trace_records(path: &Path) -> Vec<Value> {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read trace file {}: {error}", path.display()));
    raw.lines()
        .filter_map(|line| {
            if line.trim().is_empty() {
                return None;
            }
            let value = serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("invalid JSON trace line: {error}; line={line}"));
            (value["record_type"] == "orchestrator_route_trace_v1").then_some(value)
        })
        .collect::<Vec<_>>()
}

fn selected_route_roles(path: &Path) -> Vec<String> {
    trace_records(path)
        .into_iter()
        .filter(|record| record["event"] == "route-selected")
        .filter_map(|record| {
            record["selected_role"]
                .as_str()
                .map(ToString::to_string)
                .or_else(|| record["role"].as_str().map(ToString::to_string))
        })
        .collect::<Vec<_>>()
}

fn trace_contains_fallback_reason(path: &Path, phase: &str, reason: &str) -> bool {
    trace_records(path).into_iter().any(|record| {
        record["event"] == "fallback"
            && record["phase"] == phase
            && record["decision"] == "retry"
            && record["reason"] == reason
    })
}

fn latest_phase_marker(body: &str) -> Option<(usize, &'static str)> {
    [
        ("planner", "ORCHESTRATOR_PLANNER_PHASE"),
        ("delegated-step", "ORCHESTRATOR_DELEGATED_STEP_PHASE"),
        ("consolidation", "ORCHESTRATOR_CONSOLIDATION_PHASE"),
        ("execution", "ORCHESTRATOR_EXECUTION_PHASE"),
    ]
    .into_iter()
    .filter_map(|(phase, marker)| body.rfind(marker).map(|position| (position, phase)))
    .max_by_key(|(position, _)| *position)
}

fn detect_latest_phase(body: &str) -> Option<&'static str> {
    latest_phase_marker(body).map(|(_, phase)| phase)
}

fn detect_latest_role_for_phase<'a>(body: &str, roles: &'a [&'a str]) -> Option<&'a str> {
    let phase_start = latest_phase_marker(body)
        .map(|(position, _)| position)
        .unwrap_or_default();
    let phase_scoped_body = &body[phase_start..];
    roles
        .iter()
        .filter_map(|role| {
            let marker = format!("role={role}");
            phase_scoped_body
                .rfind(&marker)
                .map(|position| (position, *role))
        })
        .max_by_key(|(position, _)| *position)
        .map(|(_, role)| role)
}

fn completion_response(
    content: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> HttpMockResponse {
    let body = json!({
        "choices": [{
            "message": {"content": content},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens
        }
    })
    .to_string();
    HttpMockResponse::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(body)
        .build()
}

fn text_response(status: u16, body: &str) -> HttpMockResponse {
    HttpMockResponse::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(body.to_string())
        .build()
}

#[test]
fn unit_orchestrator_fixture_schema_guard_accepts_v1() {
    let fixture = load_orchestrator_fixture("plan-delegate-consolidate.json");
    assert_eq!(fixture.schema_version, ORCHESTRATOR_FIXTURE_SCHEMA_VERSION);
    assert_eq!(fixture.name, "plan-delegate-consolidate");
}

#[test]
fn functional_orchestrator_plan_delegate_consolidate_flow_uses_expected_routes() {
    let fixture = load_orchestrator_fixture("plan-delegate-consolidate.json");
    assert_eq!(fixture.delegated_step_responses.len(), 2);
    let temp = tempdir().expect("tempdir");
    let route_table = temp.path().join("route-table.json");
    let telemetry_log = temp.path().join("orchestrator-trace.ndjson");
    fs::write(
        &route_table,
        r#"{
  "schema_version": 1,
  "roles": {
    "planner": {},
    "worker": {},
    "reviewer": {}
  },
  "planner": { "role": "planner" },
  "delegated": { "role": "worker" },
  "delegated_categories": {
    "verify": { "role": "reviewer" }
  },
  "review": { "role": "reviewer" }
}"#,
    )
    .expect("write route table");

    #[derive(Default)]
    struct FunctionalCallState {
        planner: usize,
        delegated_step_one: usize,
        delegated_step_two: usize,
        consolidation: usize,
    }

    let planner_response = fixture.planner_response.clone();
    let delegated_step_one_response = fixture.delegated_step_responses[0].clone();
    let delegated_step_two_response = fixture.delegated_step_responses[1].clone();
    let consolidation_response = fixture.consolidation_response.clone();
    let state = Arc::new(Mutex::new(FunctionalCallState::default()));

    let server = MockServer::start();
    let calls = Arc::clone(&state);
    let orchestrator = server.mock(move |when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.respond_with(move |req: &HttpMockRequest| {
            let body = req.body().to_string();
            let phase = detect_latest_phase(&body);
            let role = detect_latest_role_for_phase(&body, &["planner", "worker", "reviewer"]);
            let mut state = calls
                .lock()
                .expect("functional call-state lock should not be poisoned");

            match (phase, role) {
                (Some("planner"), Some("planner")) => {
                    state.planner += 1;
                    completion_response(&planner_response, 8, 8)
                }
                (Some("delegated-step"), Some("worker"))
                    if body.contains("Assigned step (1 of 2)") =>
                {
                    state.delegated_step_one += 1;
                    completion_response(&delegated_step_one_response, 6, 4)
                }
                (Some("delegated-step"), Some("reviewer"))
                    if body.contains("Assigned step (2 of 2)") =>
                {
                    state.delegated_step_two += 1;
                    completion_response(&delegated_step_two_response, 6, 4)
                }
                (Some("consolidation"), Some("reviewer")) => {
                    state.consolidation += 1;
                    completion_response(&consolidation_response, 12, 6)
                }
                _ => {
                    let detail =
                        format!("unhandled orchestrator request; phase={phase:?} role={role:?}");
                    text_response(500, &detail)
                }
            }
        });
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
        fixture.prompt.as_str(),
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-delegate-steps",
        "--orchestrator-route-table",
        route_table.to_str().expect("utf8 route-table path"),
        "--telemetry-log",
        telemetry_log.to_str().expect("utf8 telemetry path"),
        "--provider-max-retries",
        "0",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "orchestrator trace: mode=plan-first phase=consolidation delegated_steps=2",
        ))
        .stdout(predicate::str::contains(
            fixture.consolidation_response.as_str(),
        ));

    orchestrator.assert_calls(4);
    let state = state
        .lock()
        .expect("functional call-state lock should not be poisoned");
    assert_eq!(state.planner, 1);
    assert_eq!(state.delegated_step_one, 1);
    assert_eq!(state.delegated_step_two, 1);
    assert_eq!(state.consolidation, 1);
    assert_eq!(
        selected_route_roles(&telemetry_log),
        fixture.expected_route_roles
    );
}

#[test]
fn integration_orchestrator_route_fallback_recovers_from_planner_error() {
    let temp = tempdir().expect("tempdir");
    let route_table = temp.path().join("route-table-fallback.json");
    let telemetry_log = temp.path().join("orchestrator-fallback-trace.ndjson");
    fs::write(
        &route_table,
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
    )
    .expect("write route table");

    #[derive(Default)]
    struct ErrorFallbackCallState {
        planner_primary: usize,
        planner_fallback: usize,
        reviewer: usize,
    }

    let state = Arc::new(Mutex::new(ErrorFallbackCallState::default()));
    let server = MockServer::start();
    let calls = Arc::clone(&state);
    let orchestrator = server.mock(move |when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.respond_with(move |req: &HttpMockRequest| {
            let body = req.body().to_string();
            let phase = detect_latest_phase(&body);
            let role = detect_latest_role_for_phase(
                &body,
                &["planner-primary", "planner-fallback", "reviewer"],
            );
            let mut state = calls
                .lock()
                .expect("fallback call-state lock should not be poisoned");

            match (phase, role) {
                (Some("planner"), Some("planner-primary")) => {
                    state.planner_primary += 1;
                    text_response(500, "planner primary down")
                }
                (Some("planner"), Some("planner-fallback")) => {
                    state.planner_fallback += 1;
                    completion_response("1. inspect\n2. apply", 8, 6)
                }
                (Some("execution"), Some("reviewer")) => {
                    state.reviewer += 1;
                    completion_response("fallback recovered final", 8, 6)
                }
                _ => {
                    let detail =
                        format!("unhandled fallback request; phase={phase:?} role={role:?}");
                    text_response(500, &detail)
                }
            }
        });
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
        "ship feature safely",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-route-table",
        route_table.to_str().expect("utf8 route-table path"),
        "--telemetry-log",
        telemetry_log.to_str().expect("utf8 telemetry path"),
        "--provider-max-retries",
        "0",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("fallback recovered final"));

    assert!(orchestrator.calls() >= 1);
    let state = state
        .lock()
        .expect("fallback call-state lock should not be poisoned");
    assert!(
        state.planner_primary >= 1,
        "planner primary role should be attempted at least once"
    );
    assert!(
        state.planner_fallback >= 1,
        "planner fallback role should be used at least once"
    );
    assert!(
        state.reviewer >= 1,
        "reviewer role should be used at least once"
    );
    assert!(trace_contains_fallback_reason(
        &telemetry_log,
        "planner",
        "prompt_execution_error"
    ));
}

#[test]
fn regression_orchestrator_route_fallback_recovers_from_planner_timeout() {
    let temp = tempdir().expect("tempdir");
    let route_table = temp.path().join("route-table-timeout-fallback.json");
    let telemetry_log = temp.path().join("orchestrator-timeout-trace.ndjson");
    fs::write(
        &route_table,
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
    )
    .expect("write route table");

    #[derive(Default)]
    struct TimeoutFallbackCallState {
        planner_primary: usize,
        planner_fallback: usize,
        reviewer: usize,
    }

    let state = Arc::new(Mutex::new(TimeoutFallbackCallState::default()));
    let server = MockServer::start();
    let calls = Arc::clone(&state);
    let orchestrator = server.mock(move |when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.respond_with(move |req: &HttpMockRequest| {
            let body = req.body().to_string();
            let phase = detect_latest_phase(&body);
            let role = detect_latest_role_for_phase(
                &body,
                &["planner-primary", "planner-fallback", "reviewer"],
            );
            let mut state = calls
                .lock()
                .expect("timeout call-state lock should not be poisoned");

            match (phase, role) {
                (Some("planner"), Some("planner-primary")) => {
                    state.planner_primary += 1;
                    std::thread::sleep(Duration::from_millis(150));
                    completion_response("late planner response", 4, 2)
                }
                (Some("planner"), Some("planner-fallback")) => {
                    state.planner_fallback += 1;
                    completion_response("1. inspect\n2. apply", 8, 6)
                }
                (Some("execution"), Some("reviewer")) => {
                    state.reviewer += 1;
                    completion_response("timeout recovered final", 8, 6)
                }
                _ => {
                    let detail =
                        format!("unhandled timeout request; phase={phase:?} role={role:?}");
                    text_response(500, &detail)
                }
            }
        });
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
        "ship feature safely",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-route-table",
        route_table.to_str().expect("utf8 route-table path"),
        "--telemetry-log",
        telemetry_log.to_str().expect("utf8 telemetry path"),
        "--provider-max-retries",
        "0",
        "--request-timeout-ms",
        "30",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("timeout recovered final"));

    assert!(orchestrator.calls() >= 1);
    let state = state
        .lock()
        .expect("timeout call-state lock should not be poisoned");
    assert!(
        state.planner_primary >= 1,
        "planner primary role should be attempted at least once"
    );
    assert!(
        state.planner_fallback >= 1,
        "planner fallback role should be used at least once"
    );
    assert!(
        state.reviewer >= 1,
        "reviewer role should be used at least once"
    );
    assert!(trace_contains_fallback_reason(
        &telemetry_log,
        "planner",
        "prompt_execution_error"
    ));
}
