use std::{path::Path, str::FromStr};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::rpc_capabilities::{rpc_capabilities_payload, RPC_PROTOCOL_VERSION};
use crate::Cli;

pub(crate) const RPC_FRAME_SCHEMA_VERSION: u32 = 1;
const RPC_STUB_MODE: &str = "preflight";

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

pub(crate) fn dispatch_rpc_frame_file(path: &Path) -> Result<RpcResponseFrame> {
    let frame = validate_rpc_frame_file(path)?;
    dispatch_rpc_frame(&frame)
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
    let response = dispatch_rpc_frame_file(path)?;
    let payload = serde_json::to_string_pretty(&response)
        .context("failed to serialize rpc response frame")?;
    println!("{payload}");
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        dispatch_rpc_frame, dispatch_rpc_frame_file, parse_rpc_frame, validate_rpc_frame_file,
        RpcFrameKind,
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
}
