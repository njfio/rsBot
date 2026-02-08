use std::{path::Path, str::FromStr};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::rpc_capabilities::{rpc_capabilities_payload, RPC_PROTOCOL_VERSION};
use crate::Cli;

pub(crate) const RPC_FRAME_SCHEMA_VERSION: u32 = 1;
const RPC_STUB_MODE: &str = "preflight";
const RPC_ERROR_KIND: &str = "error";
const RPC_ERROR_CODE_INVALID_JSON: &str = "invalid_json";
const RPC_ERROR_CODE_UNSUPPORTED_SCHEMA: &str = "unsupported_schema";
const RPC_ERROR_CODE_UNSUPPORTED_KIND: &str = "unsupported_kind";
const RPC_ERROR_CODE_INVALID_REQUEST_ID: &str = "invalid_request_id";
const RPC_ERROR_CODE_INVALID_PAYLOAD: &str = "invalid_payload";
const RPC_ERROR_CODE_IO_ERROR: &str = "io_error";
const RPC_ERROR_CODE_INTERNAL_ERROR: &str = "internal_error";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RpcFrameKind {
    CapabilitiesRequest,
    RunStart,
    RunCancel,
}

impl RpcFrameKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::CapabilitiesRequest => "capabilities.request",
            Self::RunStart => "run.start",
            Self::RunCancel => "run.cancel",
        }
    }
}

impl FromStr for RpcFrameKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "capabilities.request" => Ok(Self::CapabilitiesRequest),
            "run.start" => Ok(Self::RunStart),
            "run.cancel" => Ok(Self::RunCancel),
            other => bail!(
                "unsupported rpc frame kind '{}'; supported kinds are capabilities.request, run.start, run.cancel",
                other
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RpcFrame {
    pub request_id: String,
    pub kind: RpcFrameKind,
    pub payload: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct RpcResponseFrame {
    pub schema_version: u32,
    pub request_id: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RpcNdjsonDispatchReport {
    pub responses: Vec<RpcResponseFrame>,
    pub processed_lines: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRpcFrame {
    schema_version: u32,
    request_id: String,
    kind: String,
    payload: Value,
}

pub(crate) fn parse_rpc_frame(raw: &str) -> Result<RpcFrame> {
    let frame =
        serde_json::from_str::<RawRpcFrame>(raw).context("failed to parse rpc frame JSON")?;
    if frame.schema_version != RPC_FRAME_SCHEMA_VERSION {
        bail!(
            "unsupported rpc frame schema: expected {}, found {}",
            RPC_FRAME_SCHEMA_VERSION,
            frame.schema_version
        );
    }
    let request_id = frame.request_id.trim();
    if request_id.is_empty() {
        bail!("rpc frame request_id must be non-empty");
    }
    let kind = RpcFrameKind::from_str(frame.kind.trim())?;
    let payload = frame
        .payload
        .as_object()
        .ok_or_else(|| anyhow!("rpc frame payload must be a JSON object"))?
        .clone();
    Ok(RpcFrame {
        request_id: request_id.to_string(),
        kind,
        payload,
    })
}

pub(crate) fn validate_rpc_frame_file(path: &Path) -> Result<RpcFrame> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read rpc frame file {}", path.display()))?;
    parse_rpc_frame(&raw)
}

pub(crate) fn dispatch_rpc_frame(frame: &RpcFrame) -> Result<RpcResponseFrame> {
    match frame.kind {
        RpcFrameKind::CapabilitiesRequest => {
            let capabilities = rpc_capabilities_payload();
            let capability_list = capabilities["capabilities"]
                .as_array()
                .cloned()
                .ok_or_else(|| anyhow!("rpc capabilities payload is missing capabilities array"))?;
            Ok(build_response_frame(
                &frame.request_id,
                "capabilities.response",
                json!({
                    "protocol_version": RPC_PROTOCOL_VERSION,
                    "capabilities": capability_list,
                }),
            ))
        }
        RpcFrameKind::RunStart => {
            let prompt =
                require_non_empty_payload_string(&frame.payload, "prompt", frame.kind.as_str())?;
            Ok(build_response_frame(
                &frame.request_id,
                "run.accepted",
                json!({
                    "status": "accepted",
                    "mode": RPC_STUB_MODE,
                    "prompt_chars": prompt.chars().count(),
                }),
            ))
        }
        RpcFrameKind::RunCancel => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            Ok(build_response_frame(
                &frame.request_id,
                "run.cancelled",
                json!({
                    "status": "cancelled",
                    "mode": RPC_STUB_MODE,
                    "run_id": run_id,
                }),
            ))
        }
    }
}

#[cfg(test)]
pub(crate) fn dispatch_rpc_frame_file(path: &Path) -> Result<RpcResponseFrame> {
    let frame = validate_rpc_frame_file(path)?;
    dispatch_rpc_frame(&frame)
}

pub(crate) fn dispatch_rpc_raw_with_error_envelope(raw: &str) -> RpcResponseFrame {
    match parse_rpc_frame(raw) {
        Ok(frame) => match dispatch_rpc_frame(&frame) {
            Ok(response) => response,
            Err(error) => build_error_response_frame(
                &frame.request_id,
                classify_rpc_error_message(&error.to_string()),
                &error.to_string(),
            ),
        },
        Err(error) => {
            let request_id =
                best_effort_request_id_from_raw(raw).unwrap_or_else(|| "unknown".to_string());
            build_error_response_frame(
                &request_id,
                classify_rpc_error_message(&error.to_string()),
                &error.to_string(),
            )
        }
    }
}

pub(crate) fn dispatch_rpc_ndjson_input(raw: &str) -> RpcNdjsonDispatchReport {
    let mut responses = Vec::new();
    let mut processed_lines = 0_usize;
    let mut error_count = 0_usize;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        processed_lines = processed_lines.saturating_add(1);
        let response = dispatch_rpc_raw_with_error_envelope(trimmed);
        if response.kind == RPC_ERROR_KIND {
            error_count = error_count.saturating_add(1);
        }
        responses.push(response);
    }

    RpcNdjsonDispatchReport {
        responses,
        processed_lines,
        error_count,
    }
}

pub(crate) fn execute_rpc_validate_frame_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.rpc_validate_frame_file.as_ref() else {
        return Ok(());
    };
    let frame = validate_rpc_frame_file(path)?;
    println!(
        "rpc frame validate: path={} request_id={} kind={} payload_keys={}",
        path.display(),
        frame.request_id,
        frame.kind.as_str(),
        frame.payload.len()
    );
    Ok(())
}

pub(crate) fn execute_rpc_dispatch_frame_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.rpc_dispatch_frame_file.as_ref() else {
        return Ok(());
    };
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            let response = build_error_response_frame(
                "unknown",
                RPC_ERROR_CODE_IO_ERROR,
                &format!(
                    "failed to read rpc frame file {}: {}",
                    path.display(),
                    error
                ),
            );
            let payload = serde_json::to_string_pretty(&response)
                .context("failed to serialize rpc response frame")?;
            println!("{payload}");
            bail!(
                "{}",
                response.payload["message"]
                    .as_str()
                    .unwrap_or("failed to read rpc frame file")
            );
        }
    };
    let response = dispatch_rpc_raw_with_error_envelope(&raw);
    let payload = serde_json::to_string_pretty(&response)
        .context("failed to serialize rpc response frame")?;
    println!("{payload}");
    if response.kind == RPC_ERROR_KIND {
        bail!(
            "{}",
            response.payload["message"]
                .as_str()
                .unwrap_or("rpc dispatch failed")
        );
    }
    Ok(())
}

pub(crate) fn execute_rpc_dispatch_ndjson_command(cli: &Cli) -> Result<()> {
    let Some(path) = cli.rpc_dispatch_ndjson_file.as_ref() else {
        return Ok(());
    };

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read rpc ndjson dispatch file {}", path.display()))?;
    let report = dispatch_rpc_ndjson_input(&raw);
    for response in &report.responses {
        let line =
            serde_json::to_string(response).context("failed to serialize rpc response frame")?;
        println!("{line}");
    }
    if report.error_count > 0 {
        bail!(
            "rpc ndjson dispatch completed with {} error frame(s)",
            report.error_count
        );
    }
    Ok(())
}

fn build_response_frame(request_id: &str, kind: &str, payload: Value) -> RpcResponseFrame {
    RpcResponseFrame {
        schema_version: RPC_FRAME_SCHEMA_VERSION,
        request_id: request_id.to_string(),
        kind: kind.to_string(),
        payload,
    }
}

fn require_non_empty_payload_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
    kind: &str,
) -> Result<String> {
    let value = payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "rpc frame kind '{}' requires non-empty payload field '{}'",
                kind,
                key
            )
        })?;
    Ok(value.to_string())
}

fn build_error_response_frame(request_id: &str, code: &str, message: &str) -> RpcResponseFrame {
    build_response_frame(
        request_id,
        RPC_ERROR_KIND,
        json!({
            "code": code,
            "message": message,
        }),
    )
}

fn classify_rpc_error_message(message: &str) -> &'static str {
    if message.contains("failed to parse rpc frame JSON") {
        RPC_ERROR_CODE_INVALID_JSON
    } else if message.contains("unsupported rpc frame schema") {
        RPC_ERROR_CODE_UNSUPPORTED_SCHEMA
    } else if message.contains("unsupported rpc frame kind") {
        RPC_ERROR_CODE_UNSUPPORTED_KIND
    } else if message.contains("rpc frame request_id must be non-empty") {
        RPC_ERROR_CODE_INVALID_REQUEST_ID
    } else if message.contains("rpc frame payload must be a JSON object")
        || message.contains("requires non-empty payload field")
    {
        RPC_ERROR_CODE_INVALID_PAYLOAD
    } else {
        RPC_ERROR_CODE_INTERNAL_ERROR
    }
}

fn best_effort_request_id_from_raw(raw: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(raw).ok()?;
    let request_id = value
        .as_object()
        .and_then(|object| object.get("request_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(request_id.to_string())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        classify_rpc_error_message, dispatch_rpc_frame, dispatch_rpc_frame_file,
        dispatch_rpc_ndjson_input, dispatch_rpc_raw_with_error_envelope, parse_rpc_frame,
        validate_rpc_frame_file, RpcFrameKind, RPC_ERROR_CODE_INVALID_JSON,
        RPC_ERROR_CODE_INVALID_PAYLOAD, RPC_ERROR_CODE_INVALID_REQUEST_ID,
        RPC_ERROR_CODE_UNSUPPORTED_KIND, RPC_ERROR_CODE_UNSUPPORTED_SCHEMA,
    };

    #[test]
    fn unit_parse_rpc_frame_accepts_supported_kind_and_payload_object() {
        let frame = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-1",
  "kind": "run.start",
  "payload": {"prompt":"hello"}
}"#,
        )
        .expect("parse frame");
        assert_eq!(frame.request_id, "req-1");
        assert_eq!(frame.kind, RpcFrameKind::RunStart);
        assert_eq!(frame.payload.len(), 1);
    }

    #[test]
    fn unit_dispatch_rpc_frame_maps_supported_kinds_to_response_envelopes() {
        let capabilities = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-cap",
  "kind": "capabilities.request",
  "payload": {}
}"#,
        )
        .expect("parse capabilities");
        let capabilities_response = dispatch_rpc_frame(&capabilities).expect("dispatch");
        assert_eq!(capabilities_response.kind, "capabilities.response");
        assert_eq!(
            capabilities_response.payload["protocol_version"].as_str(),
            Some("0.1.0")
        );

        let start = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {"prompt":"hello world"}
}"#,
        )
        .expect("parse start");
        let start_response = dispatch_rpc_frame(&start).expect("dispatch start");
        assert_eq!(start_response.kind, "run.accepted");
        assert_eq!(start_response.payload["prompt_chars"].as_u64(), Some(11));

        let cancel = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-cancel",
  "kind": "run.cancel",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse cancel");
        let cancel_response = dispatch_rpc_frame(&cancel).expect("dispatch cancel");
        assert_eq!(cancel_response.kind, "run.cancelled");
        assert_eq!(cancel_response.payload["run_id"].as_str(), Some("run-1"));
    }

    #[test]
    fn functional_validate_rpc_frame_file_reports_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let frame_path = temp.path().join("frame.json");
        std::fs::write(
            &frame_path,
            r#"{
  "schema_version": 1,
  "request_id": "req-cap",
  "kind": "capabilities.request",
  "payload": {}
}"#,
        )
        .expect("write frame");

        let frame = validate_rpc_frame_file(&frame_path).expect("validate frame");
        assert_eq!(frame.request_id, "req-cap");
        assert_eq!(frame.kind, RpcFrameKind::CapabilitiesRequest);
    }

    #[test]
    fn integration_dispatch_rpc_frame_file_returns_response_frame() {
        let temp = tempdir().expect("tempdir");
        let frame_path = temp.path().join("frame.json");
        std::fs::write(
            &frame_path,
            r#"{
  "schema_version": 1,
  "request_id": "req-dispatch",
  "kind": "run.cancel",
  "payload": {"run_id":"run-42"}
}"#,
        )
        .expect("write frame");

        let response = dispatch_rpc_frame_file(&frame_path).expect("dispatch frame");
        assert_eq!(response.request_id, "req-dispatch");
        assert_eq!(response.kind, "run.cancelled");
    }

    #[test]
    fn regression_parse_rpc_frame_rejects_unknown_kind_schema_and_payload_shape() {
        let schema_error = parse_rpc_frame(
            r#"{
  "schema_version": 9,
  "request_id": "req-2",
  "kind": "run.start",
  "payload": {}
}"#,
        )
        .expect_err("schema mismatch should fail");
        assert!(schema_error
            .to_string()
            .contains("unsupported rpc frame schema"));

        let kind_error = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-2",
  "kind": "run.unknown",
  "payload": {}
}"#,
        )
        .expect_err("unknown kind should fail");
        assert!(kind_error
            .to_string()
            .contains("unsupported rpc frame kind"));

        let payload_error = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-2",
  "kind": "run.cancel",
  "payload": []
}"#,
        )
        .expect_err("non-object payload should fail");
        assert!(payload_error
            .to_string()
            .contains("rpc frame payload must be a JSON object"));
    }

    #[test]
    fn regression_dispatch_rpc_frame_rejects_missing_required_payload_fields() {
        let start = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {}
}"#,
        )
        .expect("parse start");
        let start_error = dispatch_rpc_frame(&start).expect_err("missing prompt should fail");
        assert!(start_error
            .to_string()
            .contains("requires non-empty payload field 'prompt'"));

        let cancel = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-cancel",
  "kind": "run.cancel",
  "payload": {}
}"#,
        )
        .expect("parse cancel");
        let cancel_error = dispatch_rpc_frame(&cancel).expect_err("missing run_id should fail");
        assert!(cancel_error
            .to_string()
            .contains("requires non-empty payload field 'run_id'"));
    }

    #[test]
    fn unit_classify_rpc_error_message_maps_known_validation_classes() {
        assert_eq!(
            classify_rpc_error_message("failed to parse rpc frame JSON: expected value"),
            RPC_ERROR_CODE_INVALID_JSON
        );
        assert_eq!(
            classify_rpc_error_message("unsupported rpc frame schema: expected 1, found 2"),
            RPC_ERROR_CODE_UNSUPPORTED_SCHEMA
        );
        assert_eq!(
            classify_rpc_error_message(
                "unsupported rpc frame kind 'x'; supported kinds are capabilities.request, run.start, run.cancel"
            ),
            RPC_ERROR_CODE_UNSUPPORTED_KIND
        );
        assert_eq!(
            classify_rpc_error_message("rpc frame request_id must be non-empty"),
            RPC_ERROR_CODE_INVALID_REQUEST_ID
        );
        assert_eq!(
            classify_rpc_error_message(
                "rpc frame kind 'run.start' requires non-empty payload field 'prompt'"
            ),
            RPC_ERROR_CODE_INVALID_PAYLOAD
        );
    }

    #[test]
    fn functional_dispatch_rpc_raw_with_error_envelope_returns_structured_error() {
        let response = dispatch_rpc_raw_with_error_envelope(
            r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {}
}"#,
        );
        assert_eq!(response.request_id, "req-start");
        assert_eq!(response.kind, "error");
        assert_eq!(
            response.payload["code"].as_str(),
            Some(RPC_ERROR_CODE_INVALID_PAYLOAD)
        );
    }

    #[test]
    fn regression_dispatch_rpc_raw_with_error_envelope_handles_invalid_json() {
        let response = dispatch_rpc_raw_with_error_envelope("{");
        assert_eq!(response.request_id, "unknown");
        assert_eq!(response.kind, "error");
        assert_eq!(
            response.payload["code"].as_str(),
            Some(RPC_ERROR_CODE_INVALID_JSON)
        );
    }

    #[test]
    fn unit_dispatch_rpc_ndjson_input_preserves_order_and_counts() {
        let report = dispatch_rpc_ndjson_input(
            r#"
# comment
{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
"#,
        );
        assert_eq!(report.processed_lines, 2);
        assert_eq!(report.error_count, 0);
        assert_eq!(report.responses.len(), 2);
        assert_eq!(report.responses[0].request_id, "req-cap");
        assert_eq!(report.responses[0].kind, "capabilities.response");
        assert_eq!(report.responses[1].request_id, "req-start");
        assert_eq!(report.responses[1].kind, "run.accepted");
    }

    #[test]
    fn regression_dispatch_rpc_ndjson_input_keeps_processing_after_error() {
        let report = dispatch_rpc_ndjson_input(
            r#"
{"schema_version":1,"request_id":"req-ok","kind":"run.cancel","payload":{"run_id":"run-1"}}
not-json
{"schema_version":1,"request_id":"req-ok-2","kind":"run.start","payload":{"prompt":"x"}}
"#,
        );
        assert_eq!(report.processed_lines, 3);
        assert_eq!(report.error_count, 1);
        assert_eq!(report.responses.len(), 3);
        assert_eq!(report.responses[0].kind, "run.cancelled");
        assert_eq!(report.responses[1].kind, "error");
        assert_eq!(
            report.responses[1].payload["code"].as_str(),
            Some(RPC_ERROR_CODE_INVALID_JSON)
        );
        assert_eq!(report.responses[2].kind, "run.accepted");
    }
}
