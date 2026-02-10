use std::str::FromStr;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const GATEWAY_WS_REQUEST_SCHEMA_VERSION: u32 = 1;
pub const GATEWAY_WS_RESPONSE_SCHEMA_VERSION: u32 = 1;
pub const GATEWAY_WS_PROTOCOL_VERSION: &str = "0.1.0";
pub const GATEWAY_WS_HEARTBEAT_INTERVAL_SECONDS: u64 = 15;

const GATEWAY_WS_COMPATIBLE_REQUEST_SCHEMA_VERSIONS: [u32; 2] =
    [0, GATEWAY_WS_REQUEST_SCHEMA_VERSION];

pub const GATEWAY_WS_ERROR_CODE_INVALID_JSON: &str = "invalid_json";
pub const GATEWAY_WS_ERROR_CODE_UNSUPPORTED_SCHEMA: &str = "unsupported_schema";
pub const GATEWAY_WS_ERROR_CODE_UNSUPPORTED_KIND: &str = "unsupported_kind";
pub const GATEWAY_WS_ERROR_CODE_INVALID_REQUEST_ID: &str = "invalid_request_id";
pub const GATEWAY_WS_ERROR_CODE_INVALID_PAYLOAD: &str = "invalid_payload";
pub const GATEWAY_WS_ERROR_CODE_UNAUTHORIZED: &str = "unauthorized";
pub const GATEWAY_WS_ERROR_CODE_RATE_LIMITED: &str = "rate_limited";
pub const GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR: &str = "internal_error";

const GATEWAY_WS_REQUEST_KINDS: &[&str] = &[
    "capabilities.request",
    "gateway.status.request",
    "session.status.request",
    "session.reset.request",
    "run.lifecycle.status.request",
];

const GATEWAY_WS_RESPONSE_KINDS: &[&str] = &[
    "capabilities.response",
    "gateway.status.response",
    "session.status.response",
    "session.reset.response",
    "run.lifecycle.status.response",
    "gateway.heartbeat",
    "error",
];

const GATEWAY_WS_RUN_LIFECYCLE_EVENT_KINDS: &[&str] = &[
    "run.lifecycle.accepted",
    "run.lifecycle.cancelled",
    "run.lifecycle.completed",
    "run.lifecycle.failed",
    "run.lifecycle.timed_out",
    "run.lifecycle.status",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayWsRequestKind {
    Capabilities,
    GatewayStatus,
    SessionStatus,
    SessionReset,
    RunLifecycleStatus,
}

impl FromStr for GatewayWsRequestKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "capabilities.request" => Ok(Self::Capabilities),
            "gateway.status.request" => Ok(Self::GatewayStatus),
            "session.status.request" => Ok(Self::SessionStatus),
            "session.reset.request" => Ok(Self::SessionReset),
            "run.lifecycle.status.request" => Ok(Self::RunLifecycleStatus),
            other => bail!(
                "unsupported gateway websocket frame kind '{}'; supported kinds are capabilities.request, gateway.status.request, session.status.request, session.reset.request, run.lifecycle.status.request",
                other
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GatewayWsRequestFrame {
    pub request_id: String,
    pub kind: GatewayWsRequestKind,
    pub payload: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GatewayWsResponseFrame {
    pub schema_version: u32,
    pub request_id: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct RawGatewayWsRequestFrame {
    schema_version: u32,
    request_id: String,
    kind: String,
    payload: Value,
}

pub fn parse_gateway_ws_request_frame(raw: &str) -> Result<GatewayWsRequestFrame> {
    let frame = serde_json::from_str::<RawGatewayWsRequestFrame>(raw)
        .context("failed to parse gateway websocket frame JSON")?;
    if !GATEWAY_WS_COMPATIBLE_REQUEST_SCHEMA_VERSIONS.contains(&frame.schema_version) {
        let supported = GATEWAY_WS_COMPATIBLE_REQUEST_SCHEMA_VERSIONS
            .iter()
            .map(|version| version.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "unsupported gateway websocket frame schema: supported request schema versions are [{}], found {}",
            supported,
            frame.schema_version
        );
    }
    let request_id = frame.request_id.trim();
    if request_id.is_empty() {
        bail!("gateway websocket frame request_id must be non-empty");
    }
    let kind = GatewayWsRequestKind::from_str(frame.kind.trim())?;
    let payload = frame
        .payload
        .as_object()
        .ok_or_else(|| anyhow!("gateway websocket frame payload must be a JSON object"))?
        .clone();

    Ok(GatewayWsRequestFrame {
        request_id: request_id.to_string(),
        kind,
        payload,
    })
}

pub fn best_effort_gateway_ws_request_id(raw: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(raw).ok()?;
    let request_id = value
        .as_object()
        .and_then(|object| object.get("request_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(request_id.to_string())
}

pub fn classify_gateway_ws_parse_error(message: &str) -> &'static str {
    if message.contains("failed to parse gateway websocket frame JSON") {
        GATEWAY_WS_ERROR_CODE_INVALID_JSON
    } else if message.contains("unsupported gateway websocket frame schema") {
        GATEWAY_WS_ERROR_CODE_UNSUPPORTED_SCHEMA
    } else if message.contains("unsupported gateway websocket frame kind") {
        GATEWAY_WS_ERROR_CODE_UNSUPPORTED_KIND
    } else if message.contains("gateway websocket frame request_id must be non-empty") {
        GATEWAY_WS_ERROR_CODE_INVALID_REQUEST_ID
    } else if message.contains("gateway websocket frame payload must be a JSON object")
        || message.contains("optional payload field 'session_key'")
    {
        GATEWAY_WS_ERROR_CODE_INVALID_PAYLOAD
    } else {
        GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR
    }
}

pub fn build_gateway_ws_response_frame(
    request_id: &str,
    kind: &str,
    payload: Value,
) -> GatewayWsResponseFrame {
    GatewayWsResponseFrame {
        schema_version: GATEWAY_WS_RESPONSE_SCHEMA_VERSION,
        request_id: request_id.to_string(),
        kind: kind.to_string(),
        payload,
    }
}

pub fn build_gateway_ws_error_frame(
    request_id: &str,
    code: &str,
    message: &str,
) -> GatewayWsResponseFrame {
    build_gateway_ws_response_frame(
        request_id,
        "error",
        json!({
            "code": code,
            "message": message,
        }),
    )
}

pub fn parse_optional_session_key(
    payload: &serde_json::Map<String, Value>,
) -> Result<Option<String>> {
    let Some(value) = payload.get("session_key") else {
        return Ok(None);
    };

    let raw = value
        .as_str()
        .ok_or_else(|| anyhow!("optional payload field 'session_key' must be a string"))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("optional payload field 'session_key' must be non-empty when provided");
    }
    Ok(Some(trimmed.to_string()))
}

pub fn gateway_ws_capabilities_payload() -> Value {
    json!({
        "schema_version": GATEWAY_WS_RESPONSE_SCHEMA_VERSION,
        "protocol_version": GATEWAY_WS_PROTOCOL_VERSION,
        "response_schema_version": GATEWAY_WS_RESPONSE_SCHEMA_VERSION,
        "supported_request_schema_versions": GATEWAY_WS_COMPATIBLE_REQUEST_SCHEMA_VERSIONS,
        "request_kinds": GATEWAY_WS_REQUEST_KINDS,
        "response_kinds": GATEWAY_WS_RESPONSE_KINDS,
        "contracts": {
            "heartbeat": {
                "interval_seconds": GATEWAY_WS_HEARTBEAT_INTERVAL_SECONDS,
                "transport_events": ["ws.ping", "gateway.heartbeat"],
            },
            "session": {
                "session_key_field": "session_key",
                "defaults_to": "default",
                "status_response_kind": "session.status.response",
                "reset_response_kind": "session.reset.response",
            },
            "run_lifecycle": {
                "event_kinds": GATEWAY_WS_RUN_LIFECYCLE_EVENT_KINDS,
                "status_request_kind": "run.lifecycle.status.request",
                "status_response_kind": "run.lifecycle.status.response",
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    use anyhow::{bail, Context, Result};
    use serde::Deserialize;
    use serde_json::Value;

    use super::{
        best_effort_gateway_ws_request_id, build_gateway_ws_error_frame,
        build_gateway_ws_response_frame, classify_gateway_ws_parse_error,
        gateway_ws_capabilities_payload, parse_gateway_ws_request_frame,
        parse_optional_session_key, GatewayWsRequestKind, GatewayWsResponseFrame,
        GATEWAY_WS_ERROR_CODE_INVALID_JSON, GATEWAY_WS_ERROR_CODE_INVALID_PAYLOAD,
        GATEWAY_WS_ERROR_CODE_UNSUPPORTED_SCHEMA, GATEWAY_WS_RESPONSE_SCHEMA_VERSION,
    };

    const GATEWAY_WS_SCHEMA_COMPAT_FIXTURE_SCHEMA_VERSION: u32 = 1;

    #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum GatewayWsCompatMode {
        Dispatch,
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct GatewayWsSchemaCompatFixture {
        schema_version: u32,
        name: String,
        mode: GatewayWsCompatMode,
        input_frames: Vec<String>,
        expected_processed_frames: usize,
        expected_error_count: usize,
        expected_responses: Vec<Value>,
    }

    #[derive(Debug, Default)]
    struct FixtureControlRuntime {
        sessions: BTreeMap<String, bool>,
    }

    impl FixtureControlRuntime {
        fn new() -> Self {
            let mut sessions = BTreeMap::new();
            sessions.insert("default".to_string(), true);
            Self { sessions }
        }
    }

    fn gateway_ws_schema_compat_fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("gateway-ws-protocol")
            .join(name)
    }

    fn parse_gateway_ws_schema_compat_fixture(raw: &str) -> Result<GatewayWsSchemaCompatFixture> {
        let fixture = serde_json::from_str::<GatewayWsSchemaCompatFixture>(raw)
            .context("failed to parse gateway websocket schema compatibility fixture")?;
        validate_gateway_ws_schema_compat_fixture(&fixture)?;
        Ok(fixture)
    }

    fn load_gateway_ws_schema_compat_fixture(name: &str) -> GatewayWsSchemaCompatFixture {
        let path = gateway_ws_schema_compat_fixture_path(name);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        parse_gateway_ws_schema_compat_fixture(&raw)
            .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", path.display()))
    }

    fn validate_gateway_ws_schema_compat_fixture(
        fixture: &GatewayWsSchemaCompatFixture,
    ) -> Result<()> {
        if fixture.schema_version != GATEWAY_WS_SCHEMA_COMPAT_FIXTURE_SCHEMA_VERSION {
            bail!(
                "unsupported gateway websocket schema compatibility fixture schema_version {} (expected {})",
                fixture.schema_version,
                GATEWAY_WS_SCHEMA_COMPAT_FIXTURE_SCHEMA_VERSION
            );
        }
        if fixture.name.trim().is_empty() {
            bail!("gateway websocket schema compatibility fixture name cannot be empty");
        }
        if fixture.input_frames.is_empty() {
            bail!(
                "gateway websocket schema compatibility fixture '{}' must include at least one input frame",
                fixture.name
            );
        }
        if fixture.expected_responses.is_empty() {
            bail!(
                "gateway websocket schema compatibility fixture '{}' must include at least one expected response",
                fixture.name
            );
        }
        Ok(())
    }

    fn dispatch_gateway_ws_fixture_frame(
        runtime: &mut FixtureControlRuntime,
        raw: &str,
    ) -> GatewayWsResponseFrame {
        let frame = match parse_gateway_ws_request_frame(raw) {
            Ok(frame) => frame,
            Err(error) => {
                let request_id = best_effort_gateway_ws_request_id(raw)
                    .unwrap_or_else(|| "unknown-request".to_string());
                let code = classify_gateway_ws_parse_error(error.to_string().as_str());
                return build_gateway_ws_error_frame(&request_id, code, error.to_string().as_str());
            }
        };

        match frame.kind {
            GatewayWsRequestKind::Capabilities => build_gateway_ws_response_frame(
                &frame.request_id,
                "capabilities.response",
                gateway_ws_capabilities_payload(),
            ),
            GatewayWsRequestKind::GatewayStatus => build_gateway_ws_response_frame(
                &frame.request_id,
                "gateway.status.response",
                serde_json::json!({
                    "service_status": "running",
                    "transport": "websocket",
                    "authenticated": true,
                }),
            ),
            GatewayWsRequestKind::SessionStatus => {
                let session_key = parse_optional_session_key(&frame.payload)
                    .unwrap_or_else(|error| {
                        panic!("invalid fixture session.status payload: {error}")
                    })
                    .unwrap_or_else(|| "default".to_string());
                let exists = runtime.sessions.get(&session_key).copied().unwrap_or(false);
                build_gateway_ws_response_frame(
                    &frame.request_id,
                    "session.status.response",
                    serde_json::json!({
                        "session_key": session_key,
                        "exists": exists,
                        "message_count": if exists { 2 } else { 0 },
                    }),
                )
            }
            GatewayWsRequestKind::SessionReset => {
                let session_key = parse_optional_session_key(&frame.payload)
                    .unwrap_or_else(|error| {
                        panic!("invalid fixture session.reset payload: {error}")
                    })
                    .unwrap_or_else(|| "default".to_string());
                let existed = runtime.sessions.remove(&session_key).unwrap_or(false);
                build_gateway_ws_response_frame(
                    &frame.request_id,
                    "session.reset.response",
                    serde_json::json!({
                        "session_key": session_key,
                        "reset": existed,
                    }),
                )
            }
            GatewayWsRequestKind::RunLifecycleStatus => build_gateway_ws_response_frame(
                &frame.request_id,
                "run.lifecycle.status.response",
                serde_json::json!({
                    "active_runs": [],
                    "recent_events": [],
                }),
            ),
        }
    }

    fn replay_gateway_ws_schema_compat_fixture(
        fixture: &GatewayWsSchemaCompatFixture,
    ) -> (usize, usize, Vec<Value>) {
        let mut runtime = FixtureControlRuntime::new();
        let mut responses = Vec::new();
        let mut error_count = 0usize;

        match fixture.mode {
            GatewayWsCompatMode::Dispatch => {
                for raw in &fixture.input_frames {
                    let response = dispatch_gateway_ws_fixture_frame(&mut runtime, raw);
                    if response.kind == "error" {
                        error_count = error_count.saturating_add(1);
                    }
                    responses.push(
                        serde_json::to_value(response)
                            .expect("gateway websocket response frame should serialize"),
                    );
                }
            }
        }

        (fixture.input_frames.len(), error_count, responses)
    }

    #[test]
    fn unit_parse_gateway_ws_request_frame_accepts_supported_schema_versions() {
        let frame = parse_gateway_ws_request_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-status",
  "kind": "gateway.status.request",
  "payload": {}
}"#,
        )
        .expect("parse frame");
        assert_eq!(frame.request_id, "req-status");
        assert_eq!(frame.kind, GatewayWsRequestKind::GatewayStatus);

        let legacy = parse_gateway_ws_request_frame(
            r#"{
  "schema_version": 0,
  "request_id": "req-cap",
  "kind": "capabilities.request",
  "payload": {}
}"#,
        )
        .expect("parse legacy frame");
        assert_eq!(legacy.kind, GatewayWsRequestKind::Capabilities);
    }

    #[test]
    fn unit_parse_gateway_ws_request_frame_rejects_invalid_session_key_payload() {
        let frame = parse_gateway_ws_request_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-session",
  "kind": "session.status.request",
  "payload": {"session_key": ""}
}"#,
        )
        .expect("parse frame");
        let error = parse_optional_session_key(&frame.payload)
            .expect_err("empty session key should fail validation");
        assert!(error
            .to_string()
            .contains("optional payload field 'session_key' must be non-empty"));
    }

    #[test]
    fn functional_gateway_ws_capabilities_payload_is_deterministic() {
        let payload = gateway_ws_capabilities_payload();
        assert_eq!(
            payload["schema_version"].as_u64(),
            Some(GATEWAY_WS_RESPONSE_SCHEMA_VERSION as u64)
        );
        assert_eq!(
            payload["request_kinds"].as_array().map(|kinds| kinds.len()),
            Some(5)
        );
        assert_eq!(
            payload["response_kinds"]
                .as_array()
                .map(|kinds| kinds.len()),
            Some(7)
        );
        assert_eq!(
            payload["contracts"]["heartbeat"]["interval_seconds"].as_u64(),
            Some(15)
        );
        assert_eq!(
            payload["contracts"]["run_lifecycle"]["event_kinds"]
                .as_array()
                .map(|kinds| kinds.len()),
            Some(6)
        );
    }

    #[test]
    fn unit_parse_gateway_ws_schema_compat_fixture_rejects_unsupported_fixture_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "invalid",
  "mode": "dispatch",
  "input_frames": [
    "{\"schema_version\":1,\"request_id\":\"req\",\"kind\":\"capabilities.request\",\"payload\":{}}"
  ],
  "expected_processed_frames": 1,
  "expected_error_count": 0,
  "expected_responses": [
    {"schema_version":1,"request_id":"req","kind":"capabilities.response","payload":{}}
  ]
}"#;

        let error = parse_gateway_ws_schema_compat_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported gateway websocket schema compatibility fixture schema_version"));
    }

    #[test]
    fn functional_gateway_ws_schema_compat_fixture_replays_supported_controls() {
        let fixture = load_gateway_ws_schema_compat_fixture("dispatch-supported-controls.json");
        let (processed, errors, responses) = replay_gateway_ws_schema_compat_fixture(&fixture);
        assert_eq!(processed, fixture.expected_processed_frames);
        assert_eq!(errors, fixture.expected_error_count);
        assert_eq!(responses, fixture.expected_responses);
    }

    #[test]
    fn integration_gateway_ws_schema_compat_fixture_replay_is_deterministic() {
        for name in [
            "dispatch-supported-controls.json",
            "dispatch-unsupported-schema-continues.json",
            "dispatch-unknown-kind-continues.json",
        ] {
            let fixture = load_gateway_ws_schema_compat_fixture(name);
            let first = replay_gateway_ws_schema_compat_fixture(&fixture);
            let second = replay_gateway_ws_schema_compat_fixture(&fixture);
            assert_eq!(first, second);
            assert_eq!(first.0, fixture.expected_processed_frames);
            assert_eq!(first.1, fixture.expected_error_count);
            assert_eq!(first.2, fixture.expected_responses);
        }
    }

    #[test]
    fn regression_gateway_ws_schema_compat_fixture_preserves_error_contracts() {
        let unsupported =
            load_gateway_ws_schema_compat_fixture("dispatch-unsupported-schema-continues.json");
        let (_, unsupported_errors, unsupported_responses) =
            replay_gateway_ws_schema_compat_fixture(&unsupported);
        assert_eq!(unsupported_errors, 1);
        assert_eq!(unsupported_responses, unsupported.expected_responses);
        assert_eq!(unsupported_responses[0]["kind"], "error");
        assert_eq!(
            unsupported_responses[0]["payload"]["code"],
            GATEWAY_WS_ERROR_CODE_UNSUPPORTED_SCHEMA
        );

        let unknown = load_gateway_ws_schema_compat_fixture("dispatch-unknown-kind-continues.json");
        let (_, unknown_errors, unknown_responses) =
            replay_gateway_ws_schema_compat_fixture(&unknown);
        assert_eq!(unknown_errors, 1);
        assert_eq!(unknown_responses[0]["kind"], "error");
    }

    #[test]
    fn regression_gateway_ws_error_helpers_keep_request_id_for_malformed_json() {
        let missing_request_id = build_gateway_ws_error_frame(
            "unknown-request",
            GATEWAY_WS_ERROR_CODE_INVALID_JSON,
            "failed to parse gateway websocket frame JSON: expected value at line 1 column 1",
        );
        assert_eq!(missing_request_id.request_id, "unknown-request");
        assert_eq!(missing_request_id.kind, "error");
        assert_eq!(
            missing_request_id.payload["code"],
            GATEWAY_WS_ERROR_CODE_INVALID_JSON
        );

        let raw = "not-json";
        assert_eq!(best_effort_gateway_ws_request_id(raw), None);
        assert_eq!(
            classify_gateway_ws_parse_error(
                "failed to parse gateway websocket frame JSON: expected value at line 1 column 1"
            ),
            GATEWAY_WS_ERROR_CODE_INVALID_JSON
        );
        assert_eq!(
            classify_gateway_ws_parse_error(
                "optional payload field 'session_key' must be non-empty when provided"
            ),
            GATEWAY_WS_ERROR_CODE_INVALID_PAYLOAD
        );
    }
}
