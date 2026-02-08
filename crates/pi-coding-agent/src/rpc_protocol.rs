use std::{path::Path, str::FromStr};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::Cli;

pub(crate) const RPC_FRAME_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RpcFrameKind {
    CapabilitiesRequest,
    RunStart,
    RunCancel,
}

impl RpcFrameKind {
    fn as_str(&self) -> &'static str {
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
        .ok_or_else(|| anyhow::anyhow!("rpc frame payload must be a JSON object"))?
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{parse_rpc_frame, validate_rpc_frame_file, RpcFrameKind};

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
}
