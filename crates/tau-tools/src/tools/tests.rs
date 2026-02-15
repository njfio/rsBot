//! Tests for tool catalog parsing, policy wiring, and runtime edge cases.

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use proptest::prelude::*;
use tempfile::tempdir;

use super::{
    bash_profile_name, build_docker_sandbox_spec, build_spec_from_command_template,
    builtin_agent_tool_names, canonicalize_best_effort, command_available,
    evaluate_tool_approval_gate, evaluate_tool_rate_limit_gate, evaluate_tool_rbac_gate,
    is_command_allowed, is_session_candidate_path, leading_executable,
    os_sandbox_docker_network_name, os_sandbox_mode_name, os_sandbox_policy_mode_name,
    redact_secrets, resolve_sandbox_spec, truncate_bytes, AgentTool, BashCommandProfile, BashTool,
    EditTool, HttpTool, JobsCancelTool, JobsCreateTool, JobsListTool, JobsStatusTool,
    MemoryReadTool, MemorySearchTool, MemoryTreeTool, MemoryWriteTool, OsSandboxDockerNetwork,
    OsSandboxMode, OsSandboxPolicyMode, RedoTool, SessionsHistoryTool, SessionsListTool,
    SessionsSearchTool, SessionsSendTool, SessionsStatsTool, ToolBuilderTool, ToolExecutionResult,
    ToolPolicy, ToolPolicyPreset, ToolRateLimitExceededBehavior, UndoTool, WriteTool,
};
use tau_access::ApprovalAction;
use tau_ai::Message;
use tau_extensions::{discover_extension_runtime_registrations, execute_extension_registered_tool};
use tau_session::{
    navigate_session_head, session_message_preview, session_message_role, SessionRuntime,
    SessionStore,
};

fn test_policy(path: &Path) -> Arc<ToolPolicy> {
    Arc::new(ToolPolicy::new(vec![path.to_path_buf()]))
}

fn test_policy_with_memory(path: &Path) -> Arc<ToolPolicy> {
    let mut policy = ToolPolicy::new(vec![path.to_path_buf()]);
    policy.memory_state_dir = path.join(".tau/memory");
    Arc::new(policy)
}

fn test_policy_with_jobs(path: &Path) -> Arc<ToolPolicy> {
    let mut policy = ToolPolicy::new(vec![path.to_path_buf()]);
    policy.jobs_enabled = true;
    policy.jobs_state_dir = path.join(".tau/jobs");
    policy.jobs_channel_store_root = path.join(".tau/channel-store");
    policy.jobs_default_session_path = Some(path.join(".tau/sessions/default.sqlite"));
    policy.jobs_default_timeout_ms = 5_000;
    policy.jobs_max_timeout_ms = 10_000;
    Arc::new(policy)
}

fn test_policy_with_tool_builder(path: &Path) -> Arc<ToolPolicy> {
    let mut policy = ToolPolicy::new(vec![path.to_path_buf()]);
    policy.tool_builder_enabled = true;
    policy.tool_builder_output_root = path.join(".tau/generated-tools");
    policy.tool_builder_extension_root = path.join(".tau/extensions/generated");
    policy.tool_builder_max_attempts = 3;
    Arc::new(policy)
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("set executable permissions");
    }
}

fn http_response(status_line: &str, headers: &[(&str, String)], body: &str) -> String {
    let mut response = format!("HTTP/1.1 {status_line}\r\n");
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("Connection: close\r\n");
    response.push_str(format!("Content-Length: {}\r\n", body.len()).as_str());
    response.push_str("\r\n");
    response.push_str(body);
    response
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set read timeout");
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut expected_total: Option<usize> = None;

    loop {
        let read = stream.read(&mut chunk).expect("read request bytes");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if expected_total.is_none() {
            if let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = &buffer[..header_end + 4];
                let headers_text = String::from_utf8_lossy(headers);
                let content_length = headers_text
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if !name.eq_ignore_ascii_case("Content-Length") {
                            return None;
                        }
                        value.trim().parse::<usize>().ok()
                    })
                    .unwrap_or(0);
                expected_total = Some(header_end + 4 + content_length);
            }
        }

        if let Some(expected_total) = expected_total {
            if buffer.len() >= expected_total {
                break;
            }
        }
    }

    String::from_utf8_lossy(&buffer).to_string()
}

fn spawn_http_server_once(
    response: String,
) -> (String, Arc<Mutex<Option<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test http listener");
    let addr = listener.local_addr().expect("listener local addr");
    let captured_request = Arc::new(Mutex::new(None));
    let captured_request_thread = Arc::clone(&captured_request);
    let handle = thread::spawn(move || {
        let (mut stream, _peer) = listener.accept().expect("accept request");
        let request = read_http_request(&mut stream);
        *captured_request_thread
            .lock()
            .expect("capture request lock") = Some(request);
        stream
            .write_all(response.as_bytes())
            .expect("write response bytes");
        stream.flush().expect("flush response bytes");
    });
    (format!("http://{}", addr), captured_request, handle)
}

fn spawn_http_server_sequence(
    responses: Vec<String>,
) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test http listener");
    let addr = listener.local_addr().expect("listener local addr");
    let captured_requests = Arc::new(Mutex::new(Vec::new()));
    let captured_requests_thread = Arc::clone(&captured_requests);
    let handle = thread::spawn(move || {
        for response in responses {
            let (mut stream, _peer) = listener.accept().expect("accept request");
            let request = read_http_request(&mut stream);
            captured_requests_thread
                .lock()
                .expect("capture requests lock")
                .push(request);
            stream
                .write_all(response.as_bytes())
                .expect("write response bytes");
            stream.flush().expect("flush response bytes");
        }
    });
    (format!("http://{}", addr), captured_requests, handle)
}

#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_file;

#[test]
fn unit_tool_policy_hardened_preset_applies_expected_configuration() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.apply_preset(ToolPolicyPreset::Hardened);

    assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
    assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
    assert_eq!(policy.max_command_length, 1_024);
    assert_eq!(policy.max_command_output_bytes, 4_000);
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Force);
    assert_eq!(policy.os_sandbox_policy_mode, OsSandboxPolicyMode::Required);
    assert_eq!(policy.http_timeout_ms, 10_000);
    assert_eq!(policy.http_max_response_bytes, 64_000);
    assert_eq!(policy.http_max_redirects, 2);
    assert!(!policy.http_allow_http);
    assert!(!policy.http_allow_private_network);
    assert!(policy.enforce_regular_files);
    assert_eq!(policy.tool_rate_limit_max_requests, 30);
    assert_eq!(policy.tool_rate_limit_window_ms, 60_000);
    assert_eq!(
        policy.tool_rate_limit_exceeded_behavior,
        ToolRateLimitExceededBehavior::Reject
    );
}

#[test]
fn unit_tool_policy_strict_preset_requires_sandbox_policy_mode() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.apply_preset(ToolPolicyPreset::Strict);
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Auto);
    assert_eq!(policy.os_sandbox_policy_mode, OsSandboxPolicyMode::Required);
}

#[test]
fn unit_builtin_agent_tool_name_registry_includes_session_tools() {
    let names = builtin_agent_tool_names();
    assert!(names.contains(&"read"));
    assert!(names.contains(&"write"));
    assert!(names.contains(&"edit"));
    assert!(names.contains(&"memory_write"));
    assert!(names.contains(&"memory_read"));
    assert!(names.contains(&"memory_search"));
    assert!(names.contains(&"memory_tree"));
    assert!(names.contains(&"sessions_list"));
    assert!(names.contains(&"sessions_history"));
    assert!(names.contains(&"sessions_search"));
    assert!(names.contains(&"sessions_stats"));
    assert!(names.contains(&"sessions_send"));
    assert!(names.contains(&"jobs_create"));
    assert!(names.contains(&"jobs_list"));
    assert!(names.contains(&"jobs_status"));
    assert!(names.contains(&"jobs_cancel"));
    assert!(names.contains(&"undo"));
    assert!(names.contains(&"redo"));
    assert!(names.contains(&"http"));
    assert!(names.contains(&"tool_builder"));
    assert!(names.contains(&"bash"));
}

#[tokio::test]
async fn unit_tool_builder_tool_rejects_when_disabled() {
    let temp = tempdir().expect("tempdir");
    let tool = ToolBuilderTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "name": "issue_triage",
            "description": "Generate triage tool",
            "spec": "Summarize issue urgency",
        }))
        .await;
    assert!(result.is_error);
    assert_eq!(result.content["reason_code"], "tool_builder_disabled");
}

#[tokio::test]
async fn functional_tool_builder_tool_builds_wasm_artifacts() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_tool_builder(temp.path());
    let tool = ToolBuilderTool::new(policy.clone());
    let result = tool
        .execute(serde_json::json!({
            "name": "issue_triage",
            "description": "Generate triage tool",
            "spec": "Summarize issue urgency",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" }
                },
                "required": ["title"],
                "additionalProperties": false
            }
        }))
        .await;

    assert!(!result.is_error, "tool_builder error: {}", result.content);
    assert_eq!(result.content["tool_name"], "issue_triage");
    let module_path = PathBuf::from(
        result.content["module_path"]
            .as_str()
            .expect("module_path should be a string"),
    );
    let manifest_path = PathBuf::from(
        result.content["manifest_path"]
            .as_str()
            .expect("manifest_path should be a string"),
    );
    let metadata_path = PathBuf::from(
        result.content["metadata_path"]
            .as_str()
            .expect("metadata_path should be a string"),
    );
    assert!(module_path.is_file());
    assert!(manifest_path.is_file());
    assert!(metadata_path.is_file());
    let reason_codes = result.content["reason_codes"]
        .as_array()
        .expect("reason_codes should be an array");
    assert!(reason_codes
        .iter()
        .any(|value| value == "generated_tool_sandbox_validation_succeeded"));
}

#[tokio::test]
async fn integration_tool_builder_generated_tool_executes_through_extension_runtime() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_tool_builder(temp.path());
    let tool = ToolBuilderTool::new(policy.clone());
    let build_result = tool
        .execute(serde_json::json!({
            "name": "issue_triage",
            "description": "Generate triage tool",
            "spec": "Summarize issue urgency",
        }))
        .await;
    assert!(
        !build_result.is_error,
        "tool_builder build failed: {}",
        build_result.content
    );

    let registrations = discover_extension_runtime_registrations(
        &policy.tool_builder_extension_root,
        builtin_agent_tool_names(),
        &[],
    );
    assert_eq!(registrations.registered_tools.len(), 1);
    let registered = registrations
        .registered_tools
        .first()
        .expect("generated extension tool should be registered");
    assert_eq!(registered.runtime, "wasm");
    let exec_result = execute_extension_registered_tool(registered, &serde_json::json!({}))
        .expect("generated extension tool should execute");
    assert!(!exec_result.is_error);
    assert_eq!(exec_result.content["status"], "ok");
}

#[tokio::test]
async fn regression_tool_builder_tool_retries_invalid_seed_wat() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_tool_builder(temp.path());
    let tool = ToolBuilderTool::new(policy);
    let result = tool
        .execute(serde_json::json!({
            "name": "issue_triage",
            "description": "Generate triage tool",
            "spec": "Summarize issue urgency",
            "wat_source": "(module",
            "max_attempts": 3,
        }))
        .await;
    assert!(
        !result.is_error,
        "tool_builder retry failed: {}",
        result.content
    );
    let attempts = result.content["attempts"]
        .as_array()
        .expect("attempts should be an array");
    assert!(attempts.len() >= 2);
    assert_eq!(attempts[0]["reason_code"], "generated_tool_compile_failed");
}

#[test]
fn unit_tool_policy_seeds_default_protected_paths() {
    let temp = tempdir().expect("tempdir");
    let policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    let protected_paths = policy
        .protected_paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();

    assert!(protected_paths
        .iter()
        .any(|path| path.ends_with("AGENTS.md")));
    assert!(protected_paths.iter().any(|path| path.ends_with("SOUL.md")));
    assert!(protected_paths.iter().any(|path| path.ends_with("USER.md")));
    assert!(protected_paths
        .iter()
        .any(|path| path.ends_with(".tau/AGENTS.md")));
    assert!(protected_paths
        .iter()
        .any(|path| path.ends_with(".tau/SOUL.md")));
    assert!(protected_paths
        .iter()
        .any(|path| path.ends_with(".tau/USER.md")));
    assert!(protected_paths
        .iter()
        .any(|path| path.ends_with(".tau/rbac-policy.json")));
}

#[test]
fn regression_tool_approval_gate_is_noop_when_policy_is_missing() {
    let result = evaluate_tool_approval_gate(ApprovalAction::ToolWrite {
        path: "/tmp/example.txt".to_string(),
        content_bytes: 12,
    });
    assert!(result.is_none());
}

#[test]
fn regression_tool_rbac_gate_is_noop_when_policy_is_missing() {
    let result = evaluate_tool_rbac_gate(
        Some("local:operator"),
        "write",
        None,
        serde_json::json!({
            "path": "/tmp/example.txt",
            "content_bytes": 12,
        }),
    );
    assert!(result.is_none());
}

#[test]
fn unit_tool_rate_limit_gate_enforces_limit_for_principal() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.tool_rate_limit_max_requests = 1;
    policy.tool_rate_limit_window_ms = 10_000;
    policy.rbac_principal = Some("local:rate-limit-user".to_string());

    let first = evaluate_tool_rate_limit_gate(
        &policy,
        "write",
        serde_json::json!({ "path": "note.txt", "content_bytes": 5 }),
    );
    assert!(first.is_none());

    let second = evaluate_tool_rate_limit_gate(
        &policy,
        "write",
        serde_json::json!({ "path": "note.txt", "content_bytes": 5 }),
    )
    .expect("second request should be throttled");
    assert!(second.is_error);
    assert_eq!(
        second
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("rate_limit")
    );
    assert_eq!(
        second
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("rate_limit_rejected")
    );
    assert_eq!(
        second
            .content
            .get("principal")
            .and_then(serde_json::Value::as_str),
        Some("local:rate-limit-user")
    );
}

#[test]
fn unit_tool_rate_limit_gate_supports_defer_behavior() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.tool_rate_limit_max_requests = 1;
    policy.tool_rate_limit_window_ms = 10_000;
    policy.tool_rate_limit_exceeded_behavior = ToolRateLimitExceededBehavior::Defer;
    policy.rbac_principal = Some("local:defer-user".to_string());

    let _ = evaluate_tool_rate_limit_gate(
        &policy,
        "bash",
        serde_json::json!({ "command": "printf 'ok'" }),
    );
    let second = evaluate_tool_rate_limit_gate(
        &policy,
        "bash",
        serde_json::json!({ "command": "printf 'ok'" }),
    )
    .expect("second request should be deferred");

    assert_eq!(
        second
            .content
            .get("decision")
            .and_then(serde_json::Value::as_str),
        Some("defer")
    );
    assert_eq!(
        second
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("rate_limit_deferred")
    );
}

#[tokio::test]
async fn unit_http_tool_rejects_json_payload_for_get_requests() {
    let temp = tempdir().expect("tempdir");
    let tool = HttpTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "url": "https://example.com/api",
            "method": "GET",
            "json": { "invalid": true }
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("http_body_not_allowed")
    );
}

#[tokio::test]
async fn functional_http_tool_posts_json_and_returns_structured_payload() {
    let response = http_response(
        "200 OK",
        &[("Content-Type", "application/json".to_string())],
        r#"{"ok":true,"source":"test-server"}"#,
    );
    let (base_url, captured, handle) = spawn_http_server_once(response);

    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.http_allow_http = true;
    policy.http_allow_private_network = true;
    let tool = HttpTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "url": format!("{base_url}/submit"),
            "method": "POST",
            "headers": {
                "X-Test": "1"
            },
            "json": {
                "hello": "world"
            }
        }))
        .await;
    handle.join().expect("join http server thread");

    assert!(!result.is_error);
    assert_eq!(result.content["http_status"], 200);
    assert_eq!(result.content["method"], "POST");
    assert_eq!(result.content["response_json"]["ok"], true);
    assert_eq!(result.content["redirect_count"], 0);
    let request = captured
        .lock()
        .expect("captured request lock")
        .clone()
        .expect("captured request");
    assert!(request.contains("POST /submit HTTP/1.1"));
    assert!(request.contains("\"hello\":\"world\""));
}

#[tokio::test]
async fn functional_http_tool_follows_redirects_with_per_hop_ssrf_validation() {
    let first = http_response(
        "307 Temporary Redirect",
        &[("Location", "/redirected".to_string())],
        "",
    );
    let second = http_response(
        "200 OK",
        &[("Content-Type", "application/json".to_string())],
        r#"{"ok":true,"redirected":true}"#,
    );
    let (base_url, captured, handle) = spawn_http_server_sequence(vec![first, second]);

    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.http_allow_http = true;
    policy.http_allow_private_network = true;
    let tool = HttpTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "url": format!("{base_url}/start"),
            "method": "GET"
        }))
        .await;
    handle.join().expect("join http server thread");

    assert!(!result.is_error);
    assert_eq!(result.content["http_status"], 200);
    assert_eq!(result.content["redirect_count"], 1);
    assert!(result
        .content
        .get("final_url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .ends_with("/redirected"));

    let requests = captured.lock().expect("captured requests lock");
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("GET /start HTTP/1.1"));
    assert!(requests[1].contains("GET /redirected HTTP/1.1"));
}

#[tokio::test]
async fn regression_http_tool_blocks_plain_http_without_policy_override() {
    let temp = tempdir().expect("tempdir");
    let tool = HttpTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "url": "http://example.com/path",
            "method": "GET"
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("delivery_ssrf_blocked_scheme")
    );
}

#[tokio::test]
async fn regression_http_tool_blocks_metadata_redirect_even_with_private_override() {
    let response = http_response(
        "302 Found",
        &[(
            "Location",
            "http://169.254.169.254/latest/meta-data".to_string(),
        )],
        "",
    );
    let (base_url, _captured, handle) = spawn_http_server_once(response);

    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.http_allow_http = true;
    policy.http_allow_private_network = true;
    let tool = HttpTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "url": format!("{base_url}/start"),
            "method": "GET"
        }))
        .await;
    handle.join().expect("join http server thread");

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("delivery_ssrf_blocked_metadata_endpoint")
    );
}

#[tokio::test]
async fn regression_http_tool_enforces_response_byte_cap() {
    let response = http_response(
        "200 OK",
        &[("Content-Type", "text/plain".to_string())],
        "01234567890123456789",
    );
    let (base_url, _captured, handle) = spawn_http_server_once(response);

    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.http_allow_http = true;
    policy.http_allow_private_network = true;
    policy.http_max_response_bytes = 8;
    let tool = HttpTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "url": format!("{base_url}/payload"),
            "method": "GET"
        }))
        .await;
    handle.join().expect("join http server thread");

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("http_response_too_large")
    );
}

#[test]
fn unit_is_session_candidate_path_matches_expected_shapes() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let issue_path = temp.path().join(".tau/github/repo/sessions/issue-42.jsonl");
    let event_log = temp.path().join(".tau/events/runner.jsonl");
    let non_jsonl = temp.path().join(".tau/sessions/default.txt");

    assert!(is_session_candidate_path(&session_path));
    assert!(is_session_candidate_path(&issue_path));
    assert!(!is_session_candidate_path(&event_log));
    assert!(!is_session_candidate_path(&non_jsonl));
}

#[tokio::test]
async fn functional_sessions_list_tool_reports_session_inventory() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    store
        .append_messages(
            None,
            &[
                Message::system("system seed"),
                Message::user("plan the day"),
                Message::assistant_text("done"),
            ],
        )
        .expect("append messages");

    let tool = SessionsListTool::new(test_policy(temp.path()));
    let result = tool.execute(serde_json::json!({ "limit": 16 })).await;
    assert!(!result.is_error);

    assert_eq!(
        result
            .content
            .get("returned")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let sessions = result
        .content
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0]
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .ends_with(".tau/sessions/default.sqlite"));
    assert_eq!(
        sessions[0]
            .get("newest_role")
            .and_then(serde_json::Value::as_str),
        Some("assistant")
    );
}

#[tokio::test]
async fn integration_sessions_history_tool_returns_bounded_lineage() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    let head = store
        .append_messages(
            None,
            &[
                Message::system("root"),
                Message::user("step one"),
                Message::assistant_text("step two"),
                Message::user("step three"),
            ],
        )
        .expect("append messages");

    let tool = SessionsHistoryTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
            "head_id": head,
            "limit": 2
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("lineage_entries")
            .and_then(serde_json::Value::as_u64),
        Some(4)
    );
    assert_eq!(
        result
            .content
            .get("returned")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    let history = result
        .content
        .get("history")
        .and_then(serde_json::Value::as_array)
        .expect("history array");
    assert_eq!(history.len(), 2);
    assert_eq!(
        history[0].get("role").and_then(serde_json::Value::as_str),
        Some("assistant")
    );
    assert_eq!(
        history[1].get("role").and_then(serde_json::Value::as_str),
        Some("user")
    );
}

#[tokio::test]
async fn regression_sessions_history_tool_rejects_paths_outside_allowed_roots() {
    let root = tempdir().expect("root");
    let outside = tempdir().expect("outside");
    let outside_session = outside.path().join(".tau/sessions/default.sqlite");
    let mut outside_store = SessionStore::load(&outside_session).expect("load outside session");
    outside_store
        .append_messages(None, &[Message::user("outside")])
        .expect("append outside");

    let tool = SessionsHistoryTool::new(test_policy(root.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": outside_session,
            "limit": 5
        }))
        .await;
    assert!(result.is_error);
    assert!(result.content.to_string().contains("outside allowed roots"));
}

#[tokio::test]
async fn regression_sessions_history_tool_reports_malformed_session_files() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/broken.jsonl");
    if let Some(parent) = session_path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(&session_path, "not-valid-jsonl\n").expect("write malformed session");

    let tool = SessionsHistoryTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
            "limit": 5
        }))
        .await;
    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("failed to load session"));
}

#[tokio::test]
async fn unit_sessions_search_tool_rejects_invalid_role_filter() {
    let temp = tempdir().expect("tempdir");
    let tool = SessionsSearchTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "query": "retry",
            "role": "owner"
        }))
        .await;
    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("optional argument 'role' must be one of"));
}

#[tokio::test]
async fn functional_sessions_search_tool_returns_matches_for_single_path() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    store
        .append_messages(
            None,
            &[
                Message::user("retry budget"),
                Message::assistant_text("ack"),
            ],
        )
        .expect("append messages");

    let tool = SessionsSearchTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "query": "budget",
            "path": session_path,
            "limit": 10
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("sessions_scanned")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .content
            .get("matches")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .content
            .get("returned")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let rows = result
        .content
        .get("results")
        .and_then(serde_json::Value::as_array)
        .expect("results array");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("role").and_then(serde_json::Value::as_str),
        Some("user")
    );
}

#[tokio::test]
async fn integration_sessions_search_tool_scans_discovered_session_files() {
    let temp = tempdir().expect("tempdir");
    let first_session = temp.path().join(".tau/sessions/default.sqlite");
    let second_session = temp.path().join(".tau/github/repo/sessions/issue-8.jsonl");

    let mut first_store = SessionStore::load(&first_session).expect("load first");
    first_store
        .append_messages(None, &[Message::user("delta target one")])
        .expect("append first");

    let mut second_store = SessionStore::load(&second_session).expect("load second");
    second_store
        .append_messages(None, &[Message::assistant_text("delta target two")])
        .expect("append second");

    let tool = SessionsSearchTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "query": "delta",
            "limit": 10
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("sessions_scanned")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        result
            .content
            .get("matches")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    let rows = result
        .content
        .get("results")
        .and_then(serde_json::Value::as_array)
        .expect("results array");
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn regression_sessions_search_tool_rejects_paths_outside_allowed_roots() {
    let root = tempdir().expect("root");
    let outside = tempdir().expect("outside");
    let outside_session = outside.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&outside_session).expect("load outside");
    store
        .append_messages(None, &[Message::user("outside message")])
        .expect("append outside");

    let tool = SessionsSearchTool::new(test_policy(root.path()));
    let result = tool
        .execute(serde_json::json!({
            "query": "outside",
            "path": outside_session
        }))
        .await;
    assert!(result.is_error);
    assert!(result.content.to_string().contains("outside allowed roots"));
}

#[tokio::test]
async fn unit_sessions_stats_tool_rejects_invalid_limit() {
    let temp = tempdir().expect("tempdir");
    let tool = SessionsStatsTool::new(test_policy(temp.path()));
    let result = tool.execute(serde_json::json!({ "limit": 0 })).await;
    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("optional argument 'limit' must be greater than 0"));
}

#[tokio::test]
async fn functional_sessions_stats_tool_reports_single_session_metrics() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    store
        .append_messages(
            None,
            &[
                Message::system("seed"),
                Message::user("first user message"),
                Message::assistant_text("assistant reply"),
            ],
        )
        .expect("append messages");

    let tool = SessionsStatsTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("mode")
            .and_then(serde_json::Value::as_str),
        Some("single")
    );
    assert_eq!(
        result
            .content
            .get("entries")
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    assert_eq!(
        result
            .content
            .get("branch_tips")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .content
            .get("roots")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .content
            .get("max_depth")
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    let role_counts = result
        .content
        .get("role_counts")
        .and_then(serde_json::Value::as_object)
        .expect("role counts object");
    assert_eq!(
        role_counts
            .get("assistant")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        role_counts.get("user").and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[tokio::test]
async fn integration_sessions_stats_tool_aggregates_discovered_sessions() {
    let temp = tempdir().expect("tempdir");
    let first_session = temp.path().join(".tau/sessions/default.sqlite");
    let second_session = temp.path().join(".tau/github/repo/sessions/issue-91.jsonl");

    let mut first_store = SessionStore::load(&first_session).expect("load first");
    first_store
        .append_messages(None, &[Message::user("session one")])
        .expect("append first");

    let mut second_store = SessionStore::load(&second_session).expect("load second");
    second_store
        .append_messages(None, &[Message::assistant_text("session two")])
        .expect("append second");

    let tool = SessionsStatsTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "limit": 10
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("mode")
            .and_then(serde_json::Value::as_str),
        Some("aggregate")
    );
    assert_eq!(
        result
            .content
            .get("sessions_scanned")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        result
            .content
            .get("entries")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        result
            .content
            .get("branch_tips")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    let sessions = result
        .content
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .expect("sessions array");
    assert_eq!(sessions.len(), 2);
}

#[tokio::test]
async fn regression_sessions_stats_tool_rejects_paths_outside_allowed_roots() {
    let root = tempdir().expect("root");
    let outside = tempdir().expect("outside");
    let outside_session = outside.path().join(".tau/sessions/default.sqlite");
    let mut outside_store = SessionStore::load(&outside_session).expect("load outside");
    outside_store
        .append_messages(None, &[Message::user("outside stats")])
        .expect("append outside");

    let tool = SessionsStatsTool::new(test_policy(root.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": outside_session
        }))
        .await;
    assert!(result.is_error);
    assert!(result.content.to_string().contains("outside allowed roots"));
}

#[tokio::test]
async fn regression_sessions_stats_tool_skips_malformed_sessions_in_aggregate_mode() {
    let temp = tempdir().expect("tempdir");
    let valid_session = temp.path().join(".tau/sessions/default.sqlite");
    let malformed_session = temp.path().join(".tau/sessions/broken.jsonl");
    let mut store = SessionStore::load(&valid_session).expect("load valid");
    store
        .append_messages(None, &[Message::assistant_text("valid session")])
        .expect("append valid");
    if let Some(parent) = malformed_session.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(&malformed_session, "{not-jsonl\n").expect("write malformed");

    let tool = SessionsStatsTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "limit": 10
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("sessions_scanned")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .content
            .get("skipped_invalid")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[tokio::test]
async fn unit_sessions_send_tool_rejects_empty_message() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let tool = SessionsSendTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
            "message": "   ",
        }))
        .await;
    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("message must not be empty"));
}

#[tokio::test]
async fn functional_sessions_send_tool_appends_and_reports_metadata() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    store
        .append_messages(None, &[Message::user("existing")])
        .expect("append existing");
    let previous_head_id = store.head_id();

    let tool = SessionsSendTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
            "message": "handoff: finish report",
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("previous_head_id")
            .and_then(serde_json::Value::as_u64),
        previous_head_id
    );
    assert_eq!(
        result
            .content
            .get("appended_entries")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert!(result
        .content
        .get("new_head_id")
        .and_then(serde_json::Value::as_u64)
        .is_some());
}

#[tokio::test]
async fn integration_sessions_send_tool_persists_updated_session_state() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    store
        .append_messages(None, &[Message::system("root")])
        .expect("append root");

    let tool = SessionsSendTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
            "message": "delegate this follow-up",
        }))
        .await;
    assert!(!result.is_error);

    let persisted = SessionStore::load(&session_path).expect("reload session");
    assert_eq!(persisted.entries().len(), 2);
    assert_eq!(
        session_message_role(&persisted.entries()[1].message),
        "user"
    );
    assert_eq!(
        session_message_preview(&persisted.entries()[1].message),
        "delegate this follow-up"
    );
}

#[tokio::test]
async fn regression_sessions_send_tool_rejects_unknown_parent_id() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    store
        .append_messages(None, &[Message::user("seed")])
        .expect("append seed");

    let tool = SessionsSendTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
            "message": "handoff",
            "parent_id": 999999u64
        }))
        .await;
    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("failed to append handoff message"));
}

#[tokio::test]
async fn unit_undo_tool_rejects_paths_outside_allowed_roots() {
    let root = tempdir().expect("root");
    let outside = tempdir().expect("outside");
    let session_path = outside.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load outside session");
    store
        .append_messages(None, &[Message::user("outside")])
        .expect("append outside message");

    let tool = UndoTool::new(test_policy(root.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
        }))
        .await;
    assert!(result.is_error);
    assert!(result.content.to_string().contains("outside allowed roots"));
}

#[tokio::test]
async fn functional_undo_tool_rewinds_navigation_head() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    let head = store
        .append_messages(
            None,
            &[
                Message::system("root"),
                Message::user("q1"),
                Message::assistant_text("a1"),
            ],
        )
        .expect("append messages")
        .expect("head");
    let branch_target = store.entries().first().expect("root entry should exist").id;
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(head),
    };
    navigate_session_head(&mut runtime, Some(branch_target)).expect("navigate to branch target");

    let tool = UndoTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(result.content["reason_code"], "session_undo_applied");
    assert_eq!(
        result
            .content
            .get("active_head_id")
            .and_then(serde_json::Value::as_u64),
        Some(head)
    );
}

#[tokio::test]
async fn integration_redo_tool_reapplies_undone_navigation_head() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    let head = store
        .append_messages(
            None,
            &[
                Message::system("root"),
                Message::user("q1"),
                Message::assistant_text("a1"),
            ],
        )
        .expect("append messages")
        .expect("head");
    let branch_target = store.entries().first().expect("root entry should exist").id;
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(head),
    };
    navigate_session_head(&mut runtime, Some(branch_target)).expect("navigate to branch target");

    let undo_tool = UndoTool::new(test_policy(temp.path()));
    let undo = undo_tool
        .execute(serde_json::json!({
            "path": session_path,
        }))
        .await;
    assert!(!undo.is_error);
    assert_eq!(undo.content["reason_code"], "session_undo_applied");

    let redo_tool = RedoTool::new(test_policy(temp.path()));
    let redo = redo_tool
        .execute(serde_json::json!({
            "path": session_path,
        }))
        .await;
    assert!(!redo.is_error);
    assert_eq!(redo.content["reason_code"], "session_redo_applied");
    assert_eq!(
        redo.content
            .get("active_head_id")
            .and_then(serde_json::Value::as_u64),
        Some(branch_target)
    );
}

#[tokio::test]
async fn regression_undo_tool_reports_empty_navigation_history() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join(".tau/sessions/default.sqlite");
    let mut store = SessionStore::load(&session_path).expect("load session");
    store
        .append_messages(None, &[Message::user("seed")])
        .expect("append seed");

    let tool = UndoTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": session_path,
        }))
        .await;
    assert!(result.is_error);
    assert_eq!(result.content["reason_code"], "session_undo_empty_stack");
    assert!(result
        .content
        .get("summary")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .contains("undo unavailable"));
}

#[tokio::test]
async fn unit_memory_write_tool_rejects_empty_summary() {
    let temp = tempdir().expect("tempdir");
    let tool = MemoryWriteTool::new(test_policy_with_memory(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "summary": "   ",
        }))
        .await;
    assert!(result.is_error);
    assert_eq!(result.content["reason_code"], "memory_empty_summary");
}

#[tokio::test]
async fn functional_memory_write_and_read_tools_round_trip_record() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_memory(temp.path());
    let write_tool = MemoryWriteTool::new(policy.clone());
    let write = write_tool
        .execute(serde_json::json!({
            "memory_id": "memory-release",
            "summary": "release checklist requires smoke tests",
            "tags": ["release", "nightly"],
            "facts": ["run smoke tests first"],
            "workspace_id": "workspace-a",
            "channel_id": "deploy",
            "actor_id": "assistant",
            "source_event_key": "evt-1",
            "confidence_bps": 9000
        }))
        .await;
    assert!(!write.is_error);
    assert_eq!(write.content["memory_id"], "memory-release");

    let read_tool = MemoryReadTool::new(policy);
    let read = read_tool
        .execute(serde_json::json!({
            "memory_id": "memory-release",
            "workspace_id": "workspace-a",
            "channel_id": "deploy",
            "actor_id": "assistant"
        }))
        .await;
    assert!(!read.is_error);
    assert_eq!(read.content["found"], true);
    assert_eq!(
        read.content["summary"],
        "release checklist requires smoke tests"
    );
    assert_eq!(read.content["source_event_key"], "evt-1");
}

#[tokio::test]
async fn integration_memory_search_tool_honors_scope_filter() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_memory(temp.path());
    let write_tool = MemoryWriteTool::new(policy.clone());
    let _ = write_tool
        .execute(serde_json::json!({
            "memory_id": "memory-1",
            "summary": "tokio runtime troubleshooting",
            "workspace_id": "workspace-a",
            "channel_id": "engineering"
        }))
        .await;
    let _ = write_tool
        .execute(serde_json::json!({
            "memory_id": "memory-2",
            "summary": "pasta recipe tomato basil",
            "workspace_id": "workspace-b",
            "channel_id": "kitchen"
        }))
        .await;

    let search_tool = MemorySearchTool::new(policy);
    let result = search_tool
        .execute(serde_json::json!({
            "query": "tokio runtime async",
            "workspace_id": "workspace-a",
            "limit": 5
        }))
        .await;
    assert!(!result.is_error);
    assert_eq!(result.content["returned"], 1);
    assert_eq!(result.content["matches"][0]["memory_id"], "memory-1");
    assert_eq!(
        result.content["matches"][0]["scope"]["workspace_id"],
        "workspace-a"
    );
}

#[tokio::test]
async fn regression_memory_tree_tool_reports_unique_latest_entries() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_memory(temp.path());
    let write_tool = MemoryWriteTool::new(policy.clone());
    let _ = write_tool
        .execute(serde_json::json!({
            "memory_id": "memory-1",
            "summary": "first",
            "workspace_id": "workspace-a",
            "channel_id": "deploy",
            "actor_id": "assistant"
        }))
        .await;
    let _ = write_tool
        .execute(serde_json::json!({
            "memory_id": "memory-1",
            "summary": "second",
            "workspace_id": "workspace-a",
            "channel_id": "deploy",
            "actor_id": "assistant"
        }))
        .await;

    let tree_tool = MemoryTreeTool::new(policy);
    let tree = tree_tool.execute(serde_json::json!({})).await;
    assert!(!tree.is_error);
    assert_eq!(tree.content["total_entries"], 1);
    assert_eq!(tree.content["workspaces"][0]["entry_count"], 1);
}

#[tokio::test]
async fn integration_memory_tools_fixture_roundtrip_is_deterministic() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("memory-tools")
        .join("basic-roundtrip.json");
    let fixture_raw = fs::read_to_string(&fixture_path).expect("read memory fixture");
    let fixture: serde_json::Value =
        serde_json::from_str(&fixture_raw).expect("parse memory fixture json");

    assert_eq!(fixture["schema_version"], 1);
    let operations = fixture["operations"]
        .as_array()
        .expect("operations as array")
        .clone();
    let expectations = fixture["expectations"].clone();

    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_memory(temp.path());
    let mut search_payload = None;
    let mut read_payload = None;
    let mut tree_payload = None;

    for operation in operations {
        let tool = operation
            .get("tool")
            .and_then(serde_json::Value::as_str)
            .expect("operation tool name");
        let arguments = operation
            .get("arguments")
            .cloned()
            .expect("operation arguments");
        let result = match tool {
            "memory_write" => {
                MemoryWriteTool::new(policy.clone())
                    .execute(arguments)
                    .await
            }
            "memory_search" => {
                MemorySearchTool::new(policy.clone())
                    .execute(arguments)
                    .await
            }
            "memory_read" => MemoryReadTool::new(policy.clone()).execute(arguments).await,
            "memory_tree" => MemoryTreeTool::new(policy.clone()).execute(arguments).await,
            other => panic!("unsupported fixture tool '{other}'"),
        };
        assert!(
            !result.is_error,
            "fixture operation '{tool}' should succeed: {}",
            result.content
        );
        match tool {
            "memory_search" => search_payload = Some(result.content),
            "memory_read" => read_payload = Some(result.content),
            "memory_tree" => tree_payload = Some(result.content),
            _ => {}
        }
    }

    let search_payload = search_payload.expect("search payload");
    let read_payload = read_payload.expect("read payload");
    let tree_payload = tree_payload.expect("tree payload");
    assert_eq!(
        search_payload["matches"][0]["memory_id"],
        expectations["search_first_memory_id"]
    );
    assert_eq!(read_payload["summary"], expectations["read_summary"]);
    assert_eq!(
        tree_payload["total_entries"],
        expectations["tree_total_entries"]
    );
}

fn jobs_shell_command(script: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "cmd".to_string(),
            vec!["/C".to_string(), script.to_string()],
        )
    } else {
        (
            "sh".to_string(),
            vec!["-lc".to_string(), script.to_string()],
        )
    }
}

fn jobs_sleep_script() -> &'static str {
    if cfg!(windows) {
        "ping -n 3 127.0.0.1 >NUL"
    } else {
        "sleep 1"
    }
}

async fn wait_for_job_terminal_status(
    status_tool: &JobsStatusTool,
    job_id: &str,
) -> serde_json::Value {
    let timeout = tokio::time::Duration::from_secs(5);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let status = status_tool
            .execute(serde_json::json!({
                "job_id": job_id,
            }))
            .await;
        assert!(
            !status.is_error,
            "jobs_status should succeed: {}",
            status.content
        );
        let state = status.content["job"]["status"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if matches!(state.as_str(), "succeeded" | "failed" | "cancelled") {
            return status.content;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "job {job_id} did not reach terminal status"
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
    }
}

#[tokio::test]
async fn unit_jobs_create_tool_rejects_empty_command() {
    let temp = tempdir().expect("tempdir");
    let tool = JobsCreateTool::new(test_policy_with_jobs(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "command": "   "
        }))
        .await;
    assert!(result.is_error);
    assert_eq!(result.content["reason_code"], "jobs_invalid_arguments");
}

#[tokio::test]
async fn functional_jobs_create_and_status_tools_reach_succeeded_terminal_state() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_jobs(temp.path());
    let create_tool = JobsCreateTool::new(policy.clone());
    let status_tool = JobsStatusTool::new(policy);
    let (command, args) = jobs_shell_command("echo tau-jobs-tool");
    let created = create_tool
        .execute(serde_json::json!({
            "command": command,
            "args": args,
        }))
        .await;
    assert!(!created.is_error, "jobs_create failed: {}", created.content);
    assert_eq!(created.content["reason_code"], "job_queued");
    let job_id = created.content["job"]["job_id"]
        .as_str()
        .expect("job id")
        .to_string();

    let status = wait_for_job_terminal_status(&status_tool, &job_id).await;
    assert_eq!(status["job"]["status"], "succeeded");
    assert_eq!(status["reason_code"], "jobs_status_ok");
    assert!(status["stdout_preview"]
        .as_str()
        .unwrap_or_default()
        .contains("tau-jobs-tool"));
}

#[tokio::test]
async fn integration_jobs_cancel_and_list_tools_report_persisted_state() {
    let temp = tempdir().expect("tempdir");
    let policy = test_policy_with_jobs(temp.path());
    let create_tool = JobsCreateTool::new(policy.clone());
    let list_tool = JobsListTool::new(policy.clone());
    let cancel_tool = JobsCancelTool::new(policy.clone());
    let status_tool = JobsStatusTool::new(policy);

    let (first_command, first_args) = jobs_shell_command(jobs_sleep_script());
    let first = create_tool
        .execute(serde_json::json!({
            "command": first_command,
            "args": first_args,
        }))
        .await;
    assert!(!first.is_error, "first create: {}", first.content);

    let (second_command, second_args) = jobs_shell_command("echo second-job");
    let second = create_tool
        .execute(serde_json::json!({
            "command": second_command,
            "args": second_args,
        }))
        .await;
    assert!(!second.is_error, "second create: {}", second.content);
    let second_id = second.content["job"]["job_id"]
        .as_str()
        .expect("second id")
        .to_string();

    let cancelled = cancel_tool
        .execute(serde_json::json!({
            "job_id": second_id,
        }))
        .await;
    assert!(!cancelled.is_error, "cancel failed: {}", cancelled.content);
    assert_eq!(cancelled.content["job"]["status"], "cancelled");

    let filtered = list_tool
        .execute(serde_json::json!({
            "status": "cancelled",
            "limit": 10,
        }))
        .await;
    assert!(!filtered.is_error, "list failed: {}", filtered.content);
    assert!(filtered.content["jobs"]
        .as_array()
        .map(|jobs| jobs.iter().any(|job| job["status"] == "cancelled"))
        .unwrap_or(false));

    let first_id = first.content["job"]["job_id"]
        .as_str()
        .expect("first id")
        .to_string();
    let _ = wait_for_job_terminal_status(&status_tool, &first_id).await;
}

#[tokio::test]
async fn regression_jobs_status_tool_reports_not_found_reason_code() {
    let temp = tempdir().expect("tempdir");
    let status_tool = JobsStatusTool::new(test_policy_with_jobs(temp.path()));
    let result = status_tool
        .execute(serde_json::json!({
            "job_id": "job-does-not-exist",
        }))
        .await;
    assert!(result.is_error);
    assert_eq!(result.content["reason_code"], "job_not_found");
}

#[tokio::test]
async fn edit_tool_replaces_single_match() {
    let temp = tempdir().expect("tempdir");
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "a a a").await.expect("write file");

    let tool = EditTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": file,
            "find": "a",
            "replace": "b"
        }))
        .await;

    assert!(!result.is_error);
    let content = tokio::fs::read_to_string(temp.path().join("test.txt"))
        .await
        .expect("read file");
    assert_eq!(content, "b a a");
}

#[tokio::test]
async fn edit_tool_replaces_all_matches() {
    let temp = tempdir().expect("tempdir");
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "a a a").await.expect("write file");

    let tool = EditTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": file,
            "find": "a",
            "replace": "b",
            "all": true
        }))
        .await;

    assert!(!result.is_error);
    let content = tokio::fs::read_to_string(temp.path().join("test.txt"))
        .await
        .expect("read file");
    assert_eq!(content, "b b b");
}

#[tokio::test]
async fn regression_edit_tool_rejects_result_larger_than_write_limit() {
    let temp = tempdir().expect("tempdir");
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "a").await.expect("write file");

    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.max_file_write_bytes = 3;
    let tool = EditTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "path": file,
            "find": "a",
            "replace": "longer",
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("edited content is too large"));
}

#[tokio::test]
async fn regression_write_tool_denies_default_protected_paths() {
    let temp = tempdir().expect("tempdir");
    let protected = temp.path().join("AGENTS.md");
    let tool = WriteTool::new(test_policy(temp.path()));

    let result = tool
        .execute(serde_json::json!({
            "path": protected,
            "content": "blocked",
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("protected_path")
    );
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("protected_path_denied")
    );
}

#[tokio::test]
async fn regression_write_tool_denies_tau_identity_protected_paths() {
    let temp = tempdir().expect("tempdir");
    let protected = temp.path().join(".tau/AGENTS.md");
    let tool = WriteTool::new(test_policy(temp.path()));

    let result = tool
        .execute(serde_json::json!({
            "path": protected,
            "content": "blocked",
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("protected_path")
    );
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("protected_path_denied")
    );
}

#[tokio::test]
async fn regression_edit_tool_denies_default_protected_paths() {
    let temp = tempdir().expect("tempdir");
    let protected = temp.path().join("AGENTS.md");
    tokio::fs::write(&protected, "original")
        .await
        .expect("seed protected file");

    let tool = EditTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": protected,
            "find": "original",
            "replace": "mutated",
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("protected_path")
    );
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("protected_path_denied")
    );
}

#[tokio::test]
async fn write_tool_creates_parent_directory() {
    let temp = tempdir().expect("tempdir");
    let file = temp.path().join("nested/output.txt");

    let tool = WriteTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": file,
            "content": "hello"
        }))
        .await;

    assert!(!result.is_error);
    let content = tokio::fs::read_to_string(temp.path().join("nested/output.txt"))
        .await
        .expect("read file");
    assert_eq!(content, "hello");
}

#[tokio::test]
async fn functional_write_tool_allows_protected_paths_when_override_is_enabled() {
    let temp = tempdir().expect("tempdir");
    let protected = temp.path().join("AGENTS.md");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.allow_protected_path_mutations = true;
    let tool = WriteTool::new(Arc::new(policy));

    let result = tool
        .execute(serde_json::json!({
            "path": protected,
            "content": "allowed",
        }))
        .await;

    assert!(!result.is_error);
    let content = tokio::fs::read_to_string(temp.path().join("AGENTS.md"))
        .await
        .expect("read protected file");
    assert_eq!(content, "allowed");
}

#[tokio::test]
async fn functional_write_tool_enforces_max_file_write_bytes() {
    let temp = tempdir().expect("tempdir");
    let file = temp.path().join("too-large.txt");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.max_file_write_bytes = 4;
    let tool = WriteTool::new(Arc::new(policy));

    let result = tool
        .execute(serde_json::json!({
            "path": file,
            "content": "hello"
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("content is too large (5 bytes), limit is 4 bytes"));
}

#[cfg(unix)]
#[tokio::test]
async fn functional_write_tool_rejects_symlink_targets_by_default() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target.txt");
    tokio::fs::write(&target, "safe")
        .await
        .expect("write target");
    let symlink = temp.path().join("link.txt");
    symlink_file(&target, &symlink).expect("create symlink");

    let tool = WriteTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": symlink,
            "content": "changed"
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("symbolic link, which is denied by policy"));
}

#[tokio::test]
async fn bash_tool_runs_command() {
    let temp = tempdir().expect("tempdir");
    let tool = BashTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("stdout")
            .and_then(serde_json::Value::as_str),
        Some("ok")
    );
    assert_eq!(
        result
            .content
            .get("sandbox_mode")
            .and_then(serde_json::Value::as_str),
        Some("off")
    );
    assert_eq!(
        result
            .content
            .get("sandboxed")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        result
            .content
            .get("sandbox_backend")
            .and_then(serde_json::Value::as_str),
        Some("none")
    );
}

#[tokio::test]
async fn regression_bash_tool_required_policy_mode_fails_closed_with_reason_code() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.os_sandbox_mode = OsSandboxMode::Off;
    policy.os_sandbox_policy_mode = OsSandboxPolicyMode::Required;
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("os_sandbox_mode")
    );
    assert_eq!(
        result
            .content
            .get("reason_code")
            .and_then(serde_json::Value::as_str),
        Some("sandbox_policy_required")
    );
    assert_eq!(
        result
            .content
            .get("sandbox_mode")
            .and_then(serde_json::Value::as_str),
        Some("off")
    );
    assert_eq!(
        result
            .content
            .get("sandbox_policy_mode")
            .and_then(serde_json::Value::as_str),
        Some("required")
    );
    assert!(result
        .content
        .get("error")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|error| error.contains("policy mode 'required'")));
}

#[tokio::test]
async fn regression_bash_tool_rejects_non_directory_cwd() {
    let temp = tempdir().expect("tempdir");
    let file = temp.path().join("not-a-dir.txt");
    tokio::fs::write(&file, "x").await.expect("write file");

    let tool = BashTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": file,
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("must be a directory for this operation"));
}

#[cfg(unix)]
#[tokio::test]
async fn regression_bash_tool_rejects_symlink_cwd_when_enforced() {
    let temp = tempdir().expect("tempdir");
    let real_dir = temp.path().join("real");
    tokio::fs::create_dir_all(&real_dir)
        .await
        .expect("create real dir");
    let link_dir = temp.path().join("link");
    symlink_file(&real_dir, &link_dir).expect("create symlink");

    let tool = BashTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "command": "pwd",
            "cwd": link_dir,
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("symbolic link, which is denied by policy"));
}

#[cfg(unix)]
#[tokio::test]
async fn integration_bash_tool_allows_symlink_cwd_when_enforcement_disabled() {
    let temp = tempdir().expect("tempdir");
    let real_dir = temp.path().join("real");
    tokio::fs::create_dir_all(&real_dir)
        .await
        .expect("create real dir");
    let link_dir = temp.path().join("link");
    symlink_file(&real_dir, &link_dir).expect("create symlink");

    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.enforce_regular_files = false;
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": "pwd",
            "cwd": link_dir,
        }))
        .await;

    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("success")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[tokio::test]
async fn bash_tool_times_out_long_command() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.bash_timeout_ms = 100;
    let tool = BashTool::new(Arc::new(policy));

    let result = tool
        .execute(serde_json::json!({
            "command": "sleep 2",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("command timed out after 100 ms"));
}

#[tokio::test]
async fn functional_bash_tool_rate_limit_resets_after_window() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.bash_dry_run = true;
    policy.tool_rate_limit_max_requests = 1;
    policy.tool_rate_limit_window_ms = 30;
    policy.rbac_principal = Some("local:reset-window".to_string());
    let tool = BashTool::new(Arc::new(policy));

    let first = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(!first.is_error);

    let second = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(second.is_error);
    assert_eq!(
        second
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("rate_limit")
    );

    tokio::time::sleep(Duration::from_millis(40)).await;

    let third = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(!third.is_error);
}

#[tokio::test]
async fn integration_bash_tool_rate_limit_isolated_per_principal() {
    let temp = tempdir().expect("tempdir");
    let mut base_policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    base_policy.bash_dry_run = true;
    base_policy.tool_rate_limit_max_requests = 1;
    base_policy.tool_rate_limit_window_ms = 10_000;

    let mut alice_policy = base_policy.clone();
    alice_policy.rbac_principal = Some("local:alice".to_string());
    let alice_tool = BashTool::new(Arc::new(alice_policy));

    let mut bob_policy = base_policy.clone();
    bob_policy.rbac_principal = Some("local:bob".to_string());
    let bob_tool = BashTool::new(Arc::new(bob_policy));

    let alice_first = alice_tool
        .execute(serde_json::json!({
            "command": "printf 'a'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(!alice_first.is_error);

    let alice_second = alice_tool
        .execute(serde_json::json!({
            "command": "printf 'a'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(alice_second.is_error);
    assert_eq!(
        alice_second
            .content
            .get("principal")
            .and_then(serde_json::Value::as_str),
        Some("local:alice")
    );

    let bob_first = bob_tool
        .execute(serde_json::json!({
            "command": "printf 'b'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(!bob_first.is_error);
}

#[tokio::test]
async fn unit_bash_tool_rejects_multiline_commands_by_default() {
    let temp = tempdir().expect("tempdir");
    let tool = BashTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "command": "printf 'a'\nprintf 'b'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("multiline commands are disabled"));
}

#[tokio::test]
async fn regression_bash_tool_blocks_command_not_in_allowlist() {
    let temp = tempdir().expect("tempdir");
    let tool = BashTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "command": "python --version",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert!(result
        .content
        .to_string()
        .contains("is not allowed by 'balanced' bash profile"));
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("allowed_commands")
    );
    assert!(result.content.get("policy_trace").is_none());
}

#[tokio::test]
async fn integration_bash_tool_policy_trace_emits_deny_decision_details() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.tool_policy_trace = true;
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": "python --version",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_decision")
            .and_then(serde_json::Value::as_str),
        Some("deny")
    );
    let trace = result
        .content
        .get("policy_trace")
        .and_then(serde_json::Value::as_array)
        .expect("trace should be present for trace mode");
    assert!(!trace.is_empty());
    assert!(trace.iter().any(|step| {
        step.get("check").and_then(serde_json::Value::as_str) == Some("allowed_commands")
            && step.get("outcome").and_then(serde_json::Value::as_str) == Some("deny")
    }));
}

#[tokio::test]
async fn regression_bash_tool_rate_limit_trace_reports_throttle_details() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.tool_policy_trace = true;
    policy.bash_dry_run = true;
    policy.tool_rate_limit_max_requests = 1;
    policy.tool_rate_limit_window_ms = 10_000;
    policy.rbac_principal = Some("local:trace-throttle".to_string());
    let tool = BashTool::new(Arc::new(policy));

    let first = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(!first.is_error);

    let second = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;
    assert!(second.is_error);
    assert_eq!(
        second
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("rate_limit")
    );
    assert_eq!(
        second
            .content
            .get("policy_decision")
            .and_then(serde_json::Value::as_str),
        Some("deny")
    );
    let trace = second
        .content
        .get("policy_trace")
        .and_then(serde_json::Value::as_array)
        .expect("trace should be present for throttle");
    assert!(trace.iter().any(|step| {
        step.get("check").and_then(serde_json::Value::as_str) == Some("rate_limit")
            && step.get("outcome").and_then(serde_json::Value::as_str) == Some("deny")
    }));
}

#[cfg(unix)]
#[tokio::test]
async fn functional_bash_tool_policy_override_deny_blocks_execution() {
    let temp = tempdir().expect("tempdir");
    let extensions_root = temp.path().join("extensions");
    let extension_dir = extensions_root.join("policy-enforcer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"deny\",\"reason\":\"command denied\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let marker = temp.path().join("marker.txt");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.extension_policy_override_root = Some(extensions_root);
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": format!("printf 'x' > {}", marker.display()),
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("extension_policy_override")
    );
    assert_eq!(
        result
            .content
            .get("denied_by")
            .and_then(serde_json::Value::as_str),
        Some("policy-enforcer@1.0.0")
    );
    assert!(result
        .content
        .get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .contains("command denied"));
    assert!(!marker.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn functional_bash_tool_policy_override_missing_permission_denies_before_spawn() {
    let temp = tempdir().expect("tempdir");
    let extensions_root = temp.path().join("extensions");
    let extension_dir = extensions_root.join("missing-permission");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"allow\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "missing-permission",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let marker = temp.path().join("marker.txt");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.extension_policy_override_root = Some(extensions_root);
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": format!("printf 'x' > {}", marker.display()),
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("extension_policy_override")
    );
    assert!(result
        .content
        .get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .contains("requires 'run-commands' permission"));
    assert_eq!(
        result
            .content
            .get("permission_denied")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert!(!marker.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn integration_bash_tool_policy_override_allow_permits_execution() {
    let temp = tempdir().expect("tempdir");
    let extensions_root = temp.path().join("extensions");
    let extension_dir = extensions_root.join("policy-enforcer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"allow\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.extension_policy_override_root = Some(extensions_root);
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("stdout")
            .and_then(serde_json::Value::as_str),
        Some("ok")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn regression_bash_tool_policy_override_invalid_response_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let extensions_root = temp.path().join("extensions");
    let extension_dir = extensions_root.join("broken-policy");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":123}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "broken-policy",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let marker = temp.path().join("marker.txt");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.extension_policy_override_root = Some(extensions_root);
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": format!("printf 'x' > {}", marker.display()),
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_rule")
            .and_then(serde_json::Value::as_str),
        Some("extension_policy_override")
    );
    assert!(result
        .content
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .expect("diagnostics array")
        .iter()
        .any(|value| value
            .as_str()
            .unwrap_or_default()
            .contains("invalid response")));
    assert!(!marker.exists());
}

#[tokio::test]
async fn integration_bash_tool_dry_run_validates_without_execution() {
    let temp = tempdir().expect("tempdir");
    let marker = temp.path().join("marker.txt");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.bash_dry_run = true;
    let tool = BashTool::new(Arc::new(policy));

    let result = tool
        .execute(serde_json::json!({
            "command": format!("printf 'x' > {}", marker.display()),
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("dry_run")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        result
            .content
            .get("would_execute")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        result
            .content
            .get("policy_decision")
            .and_then(serde_json::Value::as_str),
        None
    );
    assert!(result.content.get("policy_trace").is_none());
    assert!(!marker.exists());
}

#[tokio::test]
async fn functional_bash_tool_trace_includes_allow_decision_for_dry_run() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.bash_dry_run = true;
    policy.tool_policy_trace = true;
    let tool = BashTool::new(Arc::new(policy));

    let result = tool
        .execute(serde_json::json!({
            "command": "printf 'ok'",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("policy_decision")
            .and_then(serde_json::Value::as_str),
        Some("allow")
    );
    let trace = result
        .content
        .get("policy_trace")
        .and_then(serde_json::Value::as_array)
        .expect("trace should be present for trace mode");
    assert!(!trace.is_empty());
}

#[tokio::test]
async fn regression_bash_tool_rejects_commands_longer_than_policy_limit() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.max_command_length = 4;
    let tool = BashTool::new(Arc::new(policy));
    let result = tool
        .execute(serde_json::json!({
            "command": "printf",
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    assert!(result.is_error);
    assert!(result.content.to_string().contains("command is too long"));
}

#[tokio::test]
async fn functional_bash_tool_does_not_inherit_sensitive_environment_variables() {
    let temp = tempdir().expect("tempdir");
    let key = "TAU_TEST_SECRET_NOT_INHERITED";
    let previous = std::env::var(key).ok();
    std::env::set_var(key, "very-secret-value");

    let tool = BashTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "command": format!("printf \"${{{key}:-missing}}\""),
            "cwd": temp.path().display().to_string(),
        }))
        .await;

    if let Some(value) = previous {
        std::env::set_var(key, value);
    } else {
        std::env::remove_var(key);
    }

    assert!(!result.is_error);
    assert_eq!(
        result
            .content
            .get("stdout")
            .and_then(serde_json::Value::as_str),
        Some("missing")
    );
}

#[tokio::test]
async fn write_tool_blocks_paths_outside_allowed_roots() {
    let temp = tempdir().expect("tempdir");
    let outside = temp
        .path()
        .parent()
        .expect("parent path")
        .join("outside.txt");

    let tool = WriteTool::new(test_policy(temp.path()));
    let result = tool
        .execute(serde_json::json!({
            "path": outside,
            "content": "data"
        }))
        .await;

    assert!(result.is_error);
    assert!(result.content.to_string().contains("outside allowed roots"));
}

#[test]
fn tool_result_text_serializes_json() {
    let result = ToolExecutionResult::ok(serde_json::json!({ "a": 1 }));
    assert!(result.as_text().contains('"'));
}

#[test]
fn unit_build_spec_from_template_injects_shell_and_command_defaults() {
    let temp = tempdir().expect("tempdir");
    let cwd = temp.path();
    let template = vec![
        "sandbox-run".to_string(),
        "--cwd".to_string(),
        "{cwd}".to_string(),
    ];
    let spec = build_spec_from_command_template(&template, "/bin/sh", "printf 'ok'", cwd)
        .expect("template should build");

    assert_eq!(spec.program, "sandbox-run");
    assert_eq!(
        spec.args,
        vec![
            "--cwd".to_string(),
            cwd.display().to_string(),
            "/bin/sh".to_string(),
            "-lc".to_string(),
            "printf 'ok'".to_string(),
        ]
    );
    assert!(spec.sandboxed);
    assert_eq!(spec.backend, "template");
}

#[test]
fn unit_build_docker_sandbox_spec_wires_limits_mounts_and_env_allowlist() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.os_sandbox_docker_enabled = true;
    policy.os_sandbox_docker_image = "debian:stable-slim".to_string();
    policy.os_sandbox_docker_network = OsSandboxDockerNetwork::Bridge;
    policy.os_sandbox_docker_memory_mb = 384;
    policy.os_sandbox_docker_cpu_limit = 1.5;
    policy.os_sandbox_docker_pids_limit = 64;
    policy.os_sandbox_docker_read_only_rootfs = true;
    policy.os_sandbox_docker_env_allowlist = vec!["TAU_TEST_INJECT".to_string()];
    std::env::set_var("TAU_TEST_INJECT", "value-1");
    let spec = build_docker_sandbox_spec(&policy, "printf 'ok'", temp.path());
    std::env::remove_var("TAU_TEST_INJECT");

    assert_eq!(spec.program, "docker");
    assert!(spec.sandboxed);
    assert_eq!(spec.backend, "docker");
    assert!(spec
        .args
        .windows(2)
        .any(|pair| { pair[0] == "--network" && pair[1] == "bridge" }));
    assert!(spec
        .args
        .windows(2)
        .any(|pair| { pair[0] == "--memory" && pair[1] == "384m" }));
    assert!(spec
        .args
        .windows(2)
        .any(|pair| { pair[0] == "--cpus" && pair[1] == "1.500" }));
    assert!(spec
        .args
        .windows(2)
        .any(|pair| { pair[0] == "--env" && pair[1] == "TAU_TEST_INJECT=value-1" }));
    assert!(spec.args.iter().any(|value| value == "--read-only"));
}

#[test]
fn regression_resolve_sandbox_spec_force_requires_launcher_or_template() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.os_sandbox_mode = OsSandboxMode::Force;

    let result = resolve_sandbox_spec(&policy, "sh", "printf 'ok'", temp.path());
    if cfg!(target_os = "linux") && command_available("bwrap") {
        let spec = result.expect("expected bwrap sandbox spec");
        assert_eq!(spec.program, "bwrap");
        assert!(spec.sandboxed);
        assert_eq!(spec.backend, "bwrap");
        return;
    }

    let error = result.expect_err("force mode should fail without a launcher");
    assert!(error.contains("mode 'force'"));
}

#[test]
fn functional_resolve_sandbox_spec_force_uses_docker_when_bwrap_missing_and_docker_enabled() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.os_sandbox_mode = OsSandboxMode::Force;
    policy.os_sandbox_docker_enabled = true;

    let result = resolve_sandbox_spec(&policy, "sh", "printf 'ok'", temp.path());

    if cfg!(target_os = "linux") && command_available("bwrap") {
        let spec = result.expect("bwrap should still satisfy force mode");
        assert_eq!(spec.backend, "bwrap");
        return;
    }
    if command_available("docker") {
        let spec = result.expect("docker backend should satisfy force mode");
        assert_eq!(spec.backend, "docker");
        return;
    }

    let error = result.expect_err("force mode should fail when no launcher is available");
    assert!(
        error.contains("Docker backend is enabled") || error.contains("mode 'force'"),
        "unexpected error: {error}"
    );
}

#[test]
fn regression_resolve_sandbox_spec_required_denies_unsandboxed_fallback() {
    let temp = tempdir().expect("tempdir");
    let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.os_sandbox_mode = OsSandboxMode::Off;
    policy.os_sandbox_policy_mode = OsSandboxPolicyMode::Required;
    let error = resolve_sandbox_spec(&policy, "sh", "printf 'ok'", temp.path())
        .expect_err("required policy must fail closed without sandbox");
    assert!(error.contains("policy mode 'required'"));
}

#[test]
fn truncate_bytes_keeps_valid_utf8_boundaries() {
    let value = "hello🙂world";
    let truncated = truncate_bytes(value, 7);
    assert!(truncated.starts_with("hello"));
    assert!(truncated.contains("<output truncated>"));
}

proptest! {
    #[test]
    fn property_truncate_bytes_always_returns_valid_utf8(input in any::<String>(), limit in 0usize..256) {
        let truncated = truncate_bytes(&input, limit);
        prop_assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
        if input.len() <= limit {
            prop_assert_eq!(truncated, input);
        } else {
            prop_assert!(truncated.contains("<output truncated>"));
        }
    }

    #[test]
    fn property_leading_executable_handles_arbitrary_shellish_strings(prefix in "[A-Za-z_][A-Za-z0-9_]{0,8}", body in any::<String>()) {
        let command = format!("{prefix}=1 {body}");
        let _ = leading_executable(&command);
    }
}

#[test]
fn redact_secrets_replaces_sensitive_env_values() {
    std::env::set_var("TEST_API_KEY", "secret-value-123");
    let redacted = redact_secrets("token=secret-value-123");
    assert_eq!(redacted, "token=[REDACTED]");
}

#[test]
fn redact_secrets_replaces_known_secret_token_patterns() {
    let redacted = redact_secrets("openai=sk-abc123abc123abc123abc123");
    assert_eq!(redacted, "openai=[REDACTED]");
}

#[test]
fn regression_redact_secrets_replaces_project_scoped_openai_tokens() {
    let redacted = redact_secrets("openai=sk-proj-AbCdEf0123456789_uvWXyZ9876543210");
    assert_eq!(redacted, "openai=[REDACTED]");
}

#[test]
fn canonicalize_best_effort_handles_non_existing_child() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("a/b/c.txt");
    let canonical = canonicalize_best_effort(&target).expect("canonicalization should work");
    assert!(canonical.ends_with("a/b/c.txt"));
}

#[test]
fn unit_leading_executable_parses_assignments_and_paths() {
    assert_eq!(
        leading_executable("FOO=1 /usr/bin/git status"),
        Some("git".to_string())
    );
    assert_eq!(
        leading_executable("BAR=baz cargo test"),
        Some("cargo".to_string())
    );
}

#[test]
fn functional_command_allowlist_supports_prefix_patterns() {
    let allowlist = vec!["git".to_string(), "cargo-*".to_string()];
    assert!(is_command_allowed("git", &allowlist));
    assert!(is_command_allowed("cargo-nextest", &allowlist));
    assert!(!is_command_allowed("python", &allowlist));
}

#[test]
fn regression_bash_profile_name_is_stable() {
    assert_eq!(
        bash_profile_name(BashCommandProfile::Permissive),
        "permissive"
    );
    assert_eq!(bash_profile_name(BashCommandProfile::Balanced), "balanced");
    assert_eq!(bash_profile_name(BashCommandProfile::Strict), "strict");
}

#[test]
fn regression_os_sandbox_mode_name_is_stable() {
    assert_eq!(os_sandbox_mode_name(OsSandboxMode::Off), "off");
    assert_eq!(os_sandbox_mode_name(OsSandboxMode::Auto), "auto");
    assert_eq!(os_sandbox_mode_name(OsSandboxMode::Force), "force");
}

#[test]
fn regression_os_sandbox_policy_mode_name_is_stable() {
    assert_eq!(
        os_sandbox_policy_mode_name(OsSandboxPolicyMode::BestEffort),
        "best-effort"
    );
    assert_eq!(
        os_sandbox_policy_mode_name(OsSandboxPolicyMode::Required),
        "required"
    );
}

#[test]
fn regression_os_sandbox_docker_network_name_is_stable() {
    assert_eq!(
        os_sandbox_docker_network_name(OsSandboxDockerNetwork::None),
        "none"
    );
    assert_eq!(
        os_sandbox_docker_network_name(OsSandboxDockerNetwork::Bridge),
        "bridge"
    );
    assert_eq!(
        os_sandbox_docker_network_name(OsSandboxDockerNetwork::Host),
        "host"
    );
}
