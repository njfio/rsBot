//! CLI integration coverage for MCP server mode JSON-RPC roundtrips.

use std::{
    path::{Path, PathBuf},
    process::Output,
};

use serde::Deserialize;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};
use tau_ai::Message;
use tau_session::SessionStore;

use super::*;

const MCP_ROUNDTRIP_FIXTURE_SCHEMA_VERSION: u32 = 1;
const MCP_JSONRPC_VERSION: &str = "2.0";
const MCP_ERROR_PARSE: i64 = -32700;

#[derive(Debug, Deserialize)]
struct McpRoundtripFixture {
    schema_version: u32,
    name: String,
    requests: Vec<Value>,
    expected_response_ids: Vec<String>,
    expected_methods: Vec<String>,
}

struct McpWorkspace {
    _temp: TempDir,
    root: PathBuf,
    session_path: PathBuf,
    skills_dir: PathBuf,
    channel_store_root: PathBuf,
}

impl McpWorkspace {
    fn new() -> Self {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let tau_root = root.join(".tau");
        let session_path = tau_root.join("sessions/default.sqlite");
        let skills_dir = tau_root.join("skills");
        let channel_store_root = tau_root.join("channel-store");

        fs::create_dir_all(session_path.parent().expect("session parent"))
            .expect("create session dir");
        fs::create_dir_all(&skills_dir).expect("create skills dir");
        fs::create_dir_all(channel_store_root.join("channels")).expect("create channel store dirs");
        let mut store = SessionStore::load(&session_path).expect("load session");
        store
            .append_messages(None, &[Message::system("mcp-session-seed")])
            .expect("seed session");
        fs::write(skills_dir.join("checklist.md"), "# Checklist\n- test\n").expect("write skill");

        Self {
            _temp: temp,
            root,
            session_path,
            skills_dir,
            channel_store_root,
        }
    }

    fn mcp_server_args(&self) -> Vec<String> {
        vec![
            "--mcp-server".to_string(),
            "--session".to_string(),
            self.session_path.display().to_string(),
            "--skills-dir".to_string(),
            self.skills_dir.display().to_string(),
            "--channel-store-root".to_string(),
            self.channel_store_root.display().to_string(),
        ]
    }
}

fn mcp_roundtrip_fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("mcp-server-roundtrip")
        .join(name)
}

fn load_mcp_roundtrip_fixture(name: &str) -> McpRoundtripFixture {
    let path = mcp_roundtrip_fixture_path(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let fixture = serde_json::from_str::<McpRoundtripFixture>(&raw)
        .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", path.display()));
    assert_eq!(
        fixture.schema_version,
        MCP_ROUNDTRIP_FIXTURE_SCHEMA_VERSION,
        "unsupported schema_version in {}",
        path.display()
    );
    fixture
}

fn jsonrpc_request_frame(id: &str, method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": MCP_JSONRPC_VERSION,
        "id": id,
        "method": method,
        "params": params,
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn encode_mcp_frames(frames: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for frame in frames {
        let body = serde_json::to_vec(frame).expect("serialize frame");
        out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        out.extend_from_slice(&body);
    }
    out
}

fn decode_mcp_frames(raw: &[u8]) -> Vec<Value> {
    let mut cursor = 0usize;
    let mut frames = Vec::new();
    while cursor < raw.len() {
        if raw[cursor..].iter().all(u8::is_ascii_whitespace) {
            break;
        }

        let header_end = find_subslice(&raw[cursor..], b"\r\n\r\n")
            .unwrap_or_else(|| panic!("missing frame header terminator in output: {raw:?}"));
        let header_raw = &raw[cursor..cursor + header_end];
        cursor += header_end + 4;
        let header = std::str::from_utf8(header_raw).expect("header utf8");
        let content_length = header
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length:")
                    .or_else(|| line.strip_prefix("content-length:"))
                    .map(str::trim)
            })
            .and_then(|value| value.parse::<usize>().ok())
            .expect("frame Content-Length header");

        assert!(
            cursor + content_length <= raw.len(),
            "frame body exceeds output bytes: cursor={} content_length={} output_len={}",
            cursor,
            content_length,
            raw.len()
        );
        let body_raw = &raw[cursor..cursor + content_length];
        cursor += content_length;
        let frame = serde_json::from_slice::<Value>(body_raw)
            .unwrap_or_else(|error| panic!("invalid JSON frame in output: {error}"));
        frames.push(frame);
    }
    frames
}

fn run_mcp_server(workspace: &McpWorkspace, extra_args: &[&str], stdin_payload: Vec<u8>) -> Output {
    let mut cmd = binary_command();
    let mut args = workspace.mcp_server_args();
    args.extend(extra_args.iter().map(|value| (*value).to_string()));
    cmd.current_dir(&workspace.root)
        .args(args)
        .write_stdin(stdin_payload);
    cmd.output().expect("run mcp server mode")
}

#[test]
fn unit_mcp_roundtrip_fixture_schema_guard_accepts_v1() {
    let fixture = load_mcp_roundtrip_fixture("initialize-tools-list-call-session.json");
    assert_eq!(fixture.schema_version, MCP_ROUNDTRIP_FIXTURE_SCHEMA_VERSION);
    assert_eq!(fixture.name, "initialize-tools-list-call-session");
}

#[test]
fn functional_mcp_server_fixture_roundtrip_initialize_list_and_call() {
    let workspace = McpWorkspace::new();
    let fixture = load_mcp_roundtrip_fixture("initialize-tools-list-call-session.json");
    assert_eq!(
        fixture.expected_methods,
        vec![
            "initialize".to_string(),
            "tools/list".to_string(),
            "tools/call".to_string()
        ]
    );

    let output = run_mcp_server(&workspace, &[], encode_mcp_frames(&fixture.requests));
    assert!(
        output.status.success(),
        "mcp server roundtrip failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let responses = decode_mcp_frames(&output.stdout);
    let ids = responses
        .iter()
        .map(|response| {
            response["id"]
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| response["id"].to_string())
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, fixture.expected_response_ids);
    let tools = responses[1]["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert!(tools.iter().any(|tool| tool["name"] == "tau.read"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "tau.context.session"));
    assert_eq!(
        responses[2]["result"]["structuredContent"]["provider"],
        "session"
    );
    assert!(responses[2]["result"]["structuredContent"]["exists"].is_boolean());
}

#[test]
fn integration_mcp_server_tools_call_emits_policy_trace_when_enabled() {
    let workspace = McpWorkspace::new();
    let requests = vec![
        jsonrpc_request_frame("req-init", "initialize", json!({})),
        jsonrpc_request_frame(
            "req-call-bash",
            "tools/call",
            json!({
                "name": "tau.bash",
                "arguments": {
                    "command": "echo mcp-trace",
                    "cwd": workspace.root.display().to_string()
                }
            }),
        ),
    ];
    let output = run_mcp_server(
        &workspace,
        &[
            "--bash-dry-run",
            "--tool-policy-trace",
            "--bash-profile",
            "permissive",
        ],
        encode_mcp_frames(&requests),
    );
    assert!(
        output.status.success(),
        "mcp trace run failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let responses = decode_mcp_frames(&output.stdout);
    assert_eq!(responses.len(), 2);
    let trace = responses[1]["result"]["structuredContent"]["policy_trace"]
        .as_array()
        .expect("policy trace array");
    assert!(!trace.is_empty());
    assert!(responses[1]["result"]["structuredContent"]["policy_decision"].is_string());
}

#[test]
fn regression_mcp_server_invalid_payload_frame_reports_parse_error() {
    let workspace = McpWorkspace::new();
    let mut payload =
        encode_mcp_frames(&[jsonrpc_request_frame("req-init", "initialize", json!({}))]);
    let malformed = b"not-json";
    payload.extend_from_slice(format!("Content-Length: {}\r\n\r\n", malformed.len()).as_bytes());
    payload.extend_from_slice(malformed);

    let output = run_mcp_server(&workspace, &[], payload);
    assert!(
        !output.status.success(),
        "expected mcp server failure for malformed payload"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mcp server completed with 1 error frame(s)"));

    let responses = decode_mcp_frames(&output.stdout);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[1]["error"]["code"], MCP_ERROR_PARSE);
    assert!(responses[1]["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("failed to parse mcp JSON frame"));
}

#[test]
fn regression_mcp_server_disconnect_mid_frame_reports_parse_error() {
    let workspace = McpWorkspace::new();
    let partial_body =
        br#"{"jsonrpc":"2.0","id":"req-disconnect","method":"initialize","params":{"a":1}"#;
    let mut payload = Vec::new();
    payload.extend_from_slice(
        format!("Content-Length: {}\r\n\r\n", partial_body.len() + 10).as_bytes(),
    );
    payload.extend_from_slice(partial_body);

    let output = run_mcp_server(&workspace, &[], payload);
    assert!(
        !output.status.success(),
        "expected mcp server failure for mid-frame disconnect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mcp server completed with 1 error frame(s)"));

    let responses = decode_mcp_frames(&output.stdout);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["error"]["code"], MCP_ERROR_PARSE);
    assert!(responses[0]["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("failed to read mcp frame body bytes"));
}

#[test]
fn regression_mcp_server_bash_timeout_surfaces_error_payload() {
    let workspace = McpWorkspace::new();
    let requests = vec![
        jsonrpc_request_frame("req-init", "initialize", json!({})),
        jsonrpc_request_frame(
            "req-timeout",
            "tools/call",
            json!({
                "name": "tau.bash",
                "arguments": {
                    "command": "sleep 2",
                    "cwd": workspace.root.display().to_string()
                }
            }),
        ),
    ];
    let output = run_mcp_server(
        &workspace,
        &["--bash-timeout-ms", "50", "--bash-profile", "permissive"],
        encode_mcp_frames(&requests),
    );
    assert!(
        output.status.success(),
        "mcp bash timeout run failed unexpectedly: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let responses = decode_mcp_frames(&output.stdout);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[1]["id"], "req-timeout");
    assert_eq!(responses[1]["result"]["isError"], true);
    assert!(responses[1]["result"]["structuredContent"]
        .to_string()
        .contains("timed out"));
}

#[test]
fn regression_mcp_server_unavailable_external_server_fails_startup() {
    let workspace = McpWorkspace::new();
    let config_path = workspace.root.join("external-mcp.json");
    fs::write(
        &config_path,
        json!({
            "schema_version": 1,
            "servers": [{
                "name": "missing",
                "command": "/path/does-not-exist/mcp-server"
            }]
        })
        .to_string(),
    )
    .expect("write external config");

    let output = run_mcp_server(
        &workspace,
        &[
            "--mcp-external-server-config",
            config_path.to_str().expect("utf8 config path"),
        ],
        Vec::new(),
    );
    assert!(
        !output.status.success(),
        "expected startup failure for unavailable external mcp server"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to spawn external mcp server 'missing'"));
}
