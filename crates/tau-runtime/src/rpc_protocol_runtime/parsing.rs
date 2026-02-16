use std::{path::Path, str::FromStr};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;

use super::{RpcFrame, RpcFrameKind, RPC_COMPATIBLE_REQUEST_SCHEMA_VERSIONS};

#[derive(Debug, Clone, Deserialize)]
struct RawRpcFrame {
    schema_version: u32,
    request_id: String,
    kind: String,
    payload: Value,
}

pub fn parse_rpc_frame_impl(raw: &str) -> Result<RpcFrame> {
    let frame =
        serde_json::from_str::<RawRpcFrame>(raw).context("failed to parse rpc frame JSON")?;
    if !RPC_COMPATIBLE_REQUEST_SCHEMA_VERSIONS.contains(&frame.schema_version) {
        let supported_versions = RPC_COMPATIBLE_REQUEST_SCHEMA_VERSIONS
            .iter()
            .map(|version| version.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "unsupported rpc frame schema: supported request schema versions are [{}], found {}",
            supported_versions,
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

pub fn validate_rpc_frame_file_impl(path: &Path) -> Result<RpcFrame> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read rpc frame file {}", path.display()))?;
    parse_rpc_frame_impl(&raw)
}
