use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{BufRead, BufReader, Write},
    path::Path,
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::rpc_capabilities::{rpc_capabilities_payload, RPC_PROTOCOL_VERSION};
use crate::Cli;

pub(crate) const RPC_FRAME_SCHEMA_VERSION: u32 = 1;
const RPC_COMPATIBLE_REQUEST_SCHEMA_VERSIONS: [u32; 2] = [0, RPC_FRAME_SCHEMA_VERSION];
const RPC_STUB_MODE: &str = "preflight";
const RPC_ERROR_KIND: &str = "error";
const RPC_ERROR_CODE_INVALID_JSON: &str = "invalid_json";
const RPC_ERROR_CODE_UNSUPPORTED_SCHEMA: &str = "unsupported_schema";
const RPC_ERROR_CODE_UNSUPPORTED_KIND: &str = "unsupported_kind";
const RPC_ERROR_CODE_INVALID_REQUEST_ID: &str = "invalid_request_id";
const RPC_ERROR_CODE_INVALID_PAYLOAD: &str = "invalid_payload";
const RPC_ERROR_CODE_IO_ERROR: &str = "io_error";
const RPC_ERROR_CODE_INTERNAL_ERROR: &str = "internal_error";
const RPC_RUN_STREAM_ASSISTANT_TEXT_KIND: &str = "run.stream.assistant_text";
const RPC_RUN_STREAM_TOOL_EVENTS_KIND: &str = "run.stream.tool_events";
const RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RpcFrameKind {
    CapabilitiesRequest,
    RunStart,
    RunCancel,
    RunComplete,
    RunFail,
    RunTimeout,
    RunStatus,
}

impl RpcFrameKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::CapabilitiesRequest => "capabilities.request",
            Self::RunStart => "run.start",
            Self::RunCancel => "run.cancel",
            Self::RunComplete => "run.complete",
            Self::RunFail => "run.fail",
            Self::RunTimeout => "run.timeout",
            Self::RunStatus => "run.status",
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
            "run.complete" => Ok(Self::RunComplete),
            "run.fail" => Ok(Self::RunFail),
            "run.timeout" => Ok(Self::RunTimeout),
            "run.status" => Ok(Self::RunStatus),
            other => bail!(
                "unsupported rpc frame kind '{}'; supported kinds are capabilities.request, run.start, run.cancel, run.complete, run.fail, run.timeout, run.status",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RpcNdjsonServeReport {
    pub processed_lines: usize,
    pub error_count: usize,
}

#[derive(Debug, Default)]
struct RpcServeSessionState {
    active_run_ids: BTreeSet<String>,
    closed_run_states: BTreeMap<String, RpcTerminalRunState>,
    closed_run_order: VecDeque<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RpcTerminalRunState {
    terminal_state: &'static str,
    reason: Option<String>,
}

impl RpcTerminalRunState {
    fn cancelled() -> Self {
        Self {
            terminal_state: "cancelled",
            reason: None,
        }
    }

    fn completed() -> Self {
        Self {
            terminal_state: "completed",
            reason: None,
        }
    }

    fn failed(reason: &str) -> Self {
        Self {
            terminal_state: "failed",
            reason: Some(reason.to_string()),
        }
    }

    fn timed_out(reason: &str) -> Self {
        Self {
            terminal_state: "timed_out",
            reason: Some(reason.to_string()),
        }
    }
}

fn forget_closed_run_state(state: &mut RpcServeSessionState, run_id: &str) {
    if state.closed_run_states.remove(run_id).is_some() {
        if let Some(position) = state
            .closed_run_order
            .iter()
            .position(|existing| existing == run_id)
        {
            state.closed_run_order.remove(position);
        }
    }
}

fn remember_closed_run_state(
    state: &mut RpcServeSessionState,
    run_id: &str,
    terminal_state: RpcTerminalRunState,
) {
    forget_closed_run_state(state, run_id);
    state
        .closed_run_states
        .insert(run_id.to_string(), terminal_state);
    state.closed_run_order.push_back(run_id.to_string());

    while state.closed_run_order.len() > RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY {
        if let Some(evicted) = state.closed_run_order.pop_front() {
            state.closed_run_states.remove(&evicted);
        }
    }
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
                    "response_schema_version": RPC_FRAME_SCHEMA_VERSION,
                    "supported_request_schema_versions": RPC_COMPATIBLE_REQUEST_SCHEMA_VERSIONS,
                    "capabilities": capability_list,
                }),
            ))
        }
        RpcFrameKind::RunStart => {
            let prompt =
                require_non_empty_payload_string(&frame.payload, "prompt", frame.kind.as_str())?;
            let run_id = resolve_run_start_run_id(&frame.payload, &frame.request_id)?;
            Ok(build_response_frame(
                &frame.request_id,
                "run.accepted",
                json!({
                    "status": "accepted",
                    "mode": RPC_STUB_MODE,
                    "prompt_chars": prompt.chars().count(),
                    "run_id": run_id,
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
                    "terminal": true,
                    "terminal_state": "cancelled",
                    "mode": RPC_STUB_MODE,
                    "run_id": run_id,
                }),
            ))
        }
        RpcFrameKind::RunComplete => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            Ok(build_response_frame(
                &frame.request_id,
                "run.completed",
                json!({
                    "status": "completed",
                    "terminal": true,
                    "terminal_state": "completed",
                    "mode": RPC_STUB_MODE,
                    "run_id": run_id,
                }),
            ))
        }
        RpcFrameKind::RunFail => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            let reason = resolve_run_fail_reason(&frame.payload)?;
            Ok(build_response_frame(
                &frame.request_id,
                "run.failed",
                json!({
                    "status": "failed",
                    "terminal": true,
                    "terminal_state": "failed",
                    "mode": RPC_STUB_MODE,
                    "run_id": run_id,
                    "reason": reason,
                }),
            ))
        }
        RpcFrameKind::RunTimeout => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            let reason = resolve_run_timeout_reason(&frame.payload)?;
            Ok(build_response_frame(
                &frame.request_id,
                "run.timed_out",
                json!({
                    "status": "timed_out",
                    "terminal": true,
                    "terminal_state": "timed_out",
                    "mode": RPC_STUB_MODE,
                    "run_id": run_id,
                    "reason": reason,
                }),
            ))
        }
        RpcFrameKind::RunStatus => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            Ok(build_response_frame(
                &frame.request_id,
                "run.status",
                json!({
                    "status": "inactive",
                    "mode": RPC_STUB_MODE,
                    "run_id": run_id,
                    "active": false,
                    "known": false,
                    "terminal": false,
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

fn dispatch_rpc_raw_with_error_envelope_for_serve(
    raw: &str,
    state: &mut RpcServeSessionState,
) -> Vec<RpcResponseFrame> {
    match parse_rpc_frame(raw) {
        Ok(frame) => match dispatch_rpc_frame_for_serve(&frame, state) {
            Ok(responses) => responses,
            Err(error) => vec![build_error_response_frame(
                &frame.request_id,
                classify_rpc_error_message(&error.to_string()),
                &error.to_string(),
            )],
        },
        Err(error) => {
            let request_id =
                best_effort_request_id_from_raw(raw).unwrap_or_else(|| "unknown".to_string());
            vec![build_error_response_frame(
                &request_id,
                classify_rpc_error_message(&error.to_string()),
                &error.to_string(),
            )]
        }
    }
}

fn dispatch_rpc_frame_for_serve(
    frame: &RpcFrame,
    state: &mut RpcServeSessionState,
) -> Result<Vec<RpcResponseFrame>> {
    match frame.kind {
        RpcFrameKind::CapabilitiesRequest => Ok(vec![dispatch_rpc_frame(frame)?]),
        RpcFrameKind::RunStart => {
            let response = dispatch_rpc_frame(frame)?;
            let run_id = response
                .payload
                .get("run_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("rpc frame kind 'run.start' response is missing run_id"))?
                .to_string();
            if !state.active_run_ids.insert(run_id.to_string()) {
                bail!(
                    "rpc frame kind 'run.start' references duplicate active run_id '{}'",
                    run_id
                );
            }
            forget_closed_run_state(state, &run_id);
            let prompt_chars = response
                .payload
                .get("prompt_chars")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    anyhow!("rpc frame kind 'run.start' response is missing prompt_chars")
                })?;
            let mut responses = vec![response];
            responses.extend(build_run_start_stream_frames(
                &frame.request_id,
                &run_id,
                prompt_chars,
            ));
            Ok(responses)
        }
        RpcFrameKind::RunCancel => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            if !state.active_run_ids.remove(&run_id) {
                bail!(
                    "rpc frame kind 'run.cancel' references unknown run_id '{}'",
                    run_id
                );
            }
            remember_closed_run_state(state, &run_id, RpcTerminalRunState::cancelled());
            let mut responses = vec![dispatch_rpc_frame(frame)?];
            responses.push(build_run_cancel_stream_frame(&frame.request_id, &run_id));
            responses.push(build_run_cancel_assistant_stream_frame(
                &frame.request_id,
                &run_id,
            ));
            Ok(responses)
        }
        RpcFrameKind::RunComplete => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            if !state.active_run_ids.remove(&run_id) {
                bail!(
                    "rpc frame kind 'run.complete' references unknown run_id '{}'",
                    run_id
                );
            }
            remember_closed_run_state(state, &run_id, RpcTerminalRunState::completed());
            let mut responses = vec![dispatch_rpc_frame(frame)?];
            responses.push(build_run_complete_stream_frame(&frame.request_id, &run_id));
            responses.push(build_run_complete_assistant_stream_frame(
                &frame.request_id,
                &run_id,
            ));
            Ok(responses)
        }
        RpcFrameKind::RunFail => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            if !state.active_run_ids.remove(&run_id) {
                bail!(
                    "rpc frame kind 'run.fail' references unknown run_id '{}'",
                    run_id
                );
            }
            let reason = resolve_run_fail_reason(&frame.payload)?;
            remember_closed_run_state(state, &run_id, RpcTerminalRunState::failed(&reason));
            let mut responses = vec![dispatch_rpc_frame(frame)?];
            responses.push(build_run_failed_stream_frame(
                &frame.request_id,
                &run_id,
                &reason,
            ));
            responses.push(build_run_failed_assistant_stream_frame(
                &frame.request_id,
                &run_id,
                &reason,
            ));
            Ok(responses)
        }
        RpcFrameKind::RunTimeout => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            if !state.active_run_ids.remove(&run_id) {
                bail!(
                    "rpc frame kind 'run.timeout' references unknown run_id '{}'",
                    run_id
                );
            }
            let reason = resolve_run_timeout_reason(&frame.payload)?;
            remember_closed_run_state(state, &run_id, RpcTerminalRunState::timed_out(&reason));
            let mut responses = vec![dispatch_rpc_frame(frame)?];
            responses.push(build_run_timeout_stream_frame(
                &frame.request_id,
                &run_id,
                &reason,
            ));
            responses.push(build_run_timeout_assistant_stream_frame(
                &frame.request_id,
                &run_id,
                &reason,
            ));
            Ok(responses)
        }
        RpcFrameKind::RunStatus => {
            let run_id =
                require_non_empty_payload_string(&frame.payload, "run_id", frame.kind.as_str())?;
            if state.active_run_ids.contains(&run_id) {
                return Ok(vec![build_response_frame(
                    &frame.request_id,
                    "run.status",
                    json!({
                        "status": "active",
                        "mode": RPC_STUB_MODE,
                        "run_id": run_id,
                        "active": true,
                        "known": true,
                        "terminal": false,
                    }),
                )]);
            }

            if let Some(closed) = state.closed_run_states.get(&run_id) {
                let mut payload = serde_json::Map::new();
                payload.insert("status".to_string(), json!(closed.terminal_state));
                payload.insert("mode".to_string(), json!(RPC_STUB_MODE));
                payload.insert("run_id".to_string(), json!(run_id));
                payload.insert("active".to_string(), json!(false));
                payload.insert("known".to_string(), json!(true));
                payload.insert("terminal".to_string(), json!(true));
                payload.insert("terminal_state".to_string(), json!(closed.terminal_state));
                if let Some(reason) = &closed.reason {
                    payload.insert("reason".to_string(), json!(reason));
                }
                return Ok(vec![build_response_frame(
                    &frame.request_id,
                    "run.status",
                    Value::Object(payload),
                )]);
            }

            Ok(vec![build_response_frame(
                &frame.request_id,
                "run.status",
                json!({
                    "status": "inactive",
                    "mode": RPC_STUB_MODE,
                    "run_id": run_id,
                    "active": false,
                    "known": false,
                    "terminal": false,
                }),
            )])
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

pub(crate) fn serve_rpc_ndjson_reader<R, W>(
    mut reader: R,
    writer: &mut W,
) -> Result<RpcNdjsonServeReport>
where
    R: BufRead,
    W: Write,
{
    let mut line = String::new();
    let mut processed_lines = 0_usize;
    let mut error_count = 0_usize;
    let mut state = RpcServeSessionState::default();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .context("failed to read rpc ndjson input line")?;
        if bytes_read == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        processed_lines = processed_lines.saturating_add(1);
        let responses = dispatch_rpc_raw_with_error_envelope_for_serve(trimmed, &mut state);
        for response in responses {
            if response.kind == RPC_ERROR_KIND {
                error_count = error_count.saturating_add(1);
            }
            serde_json::to_writer(&mut *writer, &response)
                .context("failed to serialize rpc response frame")?;
            writer
                .write_all(b"\n")
                .context("failed to write rpc response delimiter")?;
            writer
                .flush()
                .context("failed to flush rpc response line")?;
        }
    }

    Ok(RpcNdjsonServeReport {
        processed_lines,
        error_count,
    })
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

pub(crate) fn execute_rpc_serve_ndjson_command(cli: &Cli) -> Result<()> {
    if !cli.rpc_serve_ndjson {
        return Ok(());
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    let report = serve_rpc_ndjson_reader(reader, &mut writer)?;
    if report.error_count > 0 {
        bail!(
            "rpc ndjson serve completed with {} error frame(s)",
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

fn resolve_run_start_run_id(
    payload: &serde_json::Map<String, Value>,
    request_id: &str,
) -> Result<String> {
    match payload.get("run_id") {
        Some(Value::String(run_id)) => {
            let trimmed = run_id.trim();
            if trimmed.is_empty() {
                bail!(
                    "rpc frame kind 'run.start' optional payload field 'run_id' must be a non-empty string when provided"
                );
            }
            Ok(trimmed.to_string())
        }
        Some(_) => bail!(
            "rpc frame kind 'run.start' optional payload field 'run_id' must be a non-empty string when provided"
        ),
        None => Ok(format!("run-{}", request_id.trim())),
    }
}

fn resolve_run_fail_reason(payload: &serde_json::Map<String, Value>) -> Result<String> {
    match payload.get("reason") {
        Some(Value::String(reason)) => {
            let trimmed = reason.trim();
            if trimmed.is_empty() {
                bail!(
                    "rpc frame kind 'run.fail' optional payload field 'reason' must be a non-empty string when provided"
                );
            }
            Ok(trimmed.to_string())
        }
        Some(_) => bail!(
            "rpc frame kind 'run.fail' optional payload field 'reason' must be a non-empty string when provided"
        ),
        None => Ok("failed".to_string()),
    }
}

fn resolve_run_timeout_reason(payload: &serde_json::Map<String, Value>) -> Result<String> {
    match payload.get("reason") {
        Some(Value::String(reason)) => {
            let trimmed = reason.trim();
            if trimmed.is_empty() {
                bail!(
                    "rpc frame kind 'run.timeout' optional payload field 'reason' must be a non-empty string when provided"
                );
            }
            Ok(trimmed.to_string())
        }
        Some(_) => bail!(
            "rpc frame kind 'run.timeout' optional payload field 'reason' must be a non-empty string when provided"
        ),
        None => Ok("timed out".to_string()),
    }
}

fn build_run_start_stream_frames(
    request_id: &str,
    run_id: &str,
    prompt_chars: u64,
) -> Vec<RpcResponseFrame> {
    vec![
        build_response_frame(
            request_id,
            RPC_RUN_STREAM_TOOL_EVENTS_KIND,
            json!({
                "run_id": run_id,
                "event": "run.started",
                "mode": RPC_STUB_MODE,
                "sequence": 0,
            }),
        ),
        build_response_frame(
            request_id,
            RPC_RUN_STREAM_ASSISTANT_TEXT_KIND,
            json!({
                "run_id": run_id,
                "delta": format!("preflight run accepted ({} prompt chars)", prompt_chars),
                "mode": RPC_STUB_MODE,
                "sequence": 1,
                "final": false,
            }),
        ),
    ]
}

fn build_run_complete_stream_frame(request_id: &str, run_id: &str) -> RpcResponseFrame {
    build_response_frame(
        request_id,
        RPC_RUN_STREAM_TOOL_EVENTS_KIND,
        json!({
            "run_id": run_id,
            "event": "run.completed",
            "terminal": true,
            "terminal_state": "completed",
            "mode": RPC_STUB_MODE,
            "sequence": 2,
        }),
    )
}

fn build_run_complete_assistant_stream_frame(request_id: &str, run_id: &str) -> RpcResponseFrame {
    build_run_terminal_assistant_stream_frame(
        request_id,
        run_id,
        "completed",
        None,
        "run completed",
    )
}

fn build_run_cancel_stream_frame(request_id: &str, run_id: &str) -> RpcResponseFrame {
    build_response_frame(
        request_id,
        RPC_RUN_STREAM_TOOL_EVENTS_KIND,
        json!({
            "run_id": run_id,
            "event": "run.cancelled",
            "terminal": true,
            "terminal_state": "cancelled",
            "mode": RPC_STUB_MODE,
            "sequence": 2,
        }),
    )
}

fn build_run_cancel_assistant_stream_frame(request_id: &str, run_id: &str) -> RpcResponseFrame {
    build_run_terminal_assistant_stream_frame(
        request_id,
        run_id,
        "cancelled",
        None,
        "run cancelled",
    )
}

fn build_run_failed_stream_frame(request_id: &str, run_id: &str, reason: &str) -> RpcResponseFrame {
    build_response_frame(
        request_id,
        RPC_RUN_STREAM_TOOL_EVENTS_KIND,
        json!({
            "run_id": run_id,
            "event": "run.failed",
            "terminal": true,
            "terminal_state": "failed",
            "reason": reason,
            "mode": RPC_STUB_MODE,
            "sequence": 2,
        }),
    )
}

fn build_run_failed_assistant_stream_frame(
    request_id: &str,
    run_id: &str,
    reason: &str,
) -> RpcResponseFrame {
    build_run_terminal_assistant_stream_frame(
        request_id,
        run_id,
        "failed",
        Some(reason),
        &format!("run failed: {reason}"),
    )
}

fn build_run_timeout_stream_frame(
    request_id: &str,
    run_id: &str,
    reason: &str,
) -> RpcResponseFrame {
    build_response_frame(
        request_id,
        RPC_RUN_STREAM_TOOL_EVENTS_KIND,
        json!({
            "run_id": run_id,
            "event": "run.timed_out",
            "terminal": true,
            "terminal_state": "timed_out",
            "reason": reason,
            "mode": RPC_STUB_MODE,
            "sequence": 2,
        }),
    )
}

fn build_run_timeout_assistant_stream_frame(
    request_id: &str,
    run_id: &str,
    reason: &str,
) -> RpcResponseFrame {
    build_run_terminal_assistant_stream_frame(
        request_id,
        run_id,
        "timed_out",
        Some(reason),
        &format!("run timed out: {reason}"),
    )
}

fn build_run_terminal_assistant_stream_frame(
    request_id: &str,
    run_id: &str,
    terminal_state: &str,
    reason: Option<&str>,
    delta: &str,
) -> RpcResponseFrame {
    let mut payload = serde_json::Map::new();
    payload.insert("run_id".to_string(), json!(run_id));
    payload.insert("delta".to_string(), json!(delta));
    payload.insert("mode".to_string(), json!(RPC_STUB_MODE));
    payload.insert("sequence".to_string(), json!(3));
    payload.insert("final".to_string(), json!(true));
    payload.insert("terminal".to_string(), json!(true));
    payload.insert("terminal_state".to_string(), json!(terminal_state));
    if let Some(reason) = reason {
        payload.insert("reason".to_string(), json!(reason));
    }
    build_response_frame(
        request_id,
        RPC_RUN_STREAM_ASSISTANT_TEXT_KIND,
        Value::Object(payload),
    )
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
        || message.contains("optional payload field 'run_id'")
        || message.contains("optional payload field 'reason'")
        || message.contains("duplicate active run_id")
        || message.contains("references unknown run_id")
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
    use std::{
        io::Cursor,
        path::{Path, PathBuf},
    };

    use anyhow::{bail, Context, Result};
    use serde::Deserialize;
    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        classify_rpc_error_message, dispatch_rpc_frame, dispatch_rpc_frame_file,
        dispatch_rpc_frame_for_serve, dispatch_rpc_ndjson_input,
        dispatch_rpc_raw_with_error_envelope, parse_rpc_frame, serve_rpc_ndjson_reader,
        validate_rpc_frame_file, RpcFrameKind, RpcResponseFrame, RpcServeSessionState,
        RPC_ERROR_CODE_INVALID_JSON, RPC_ERROR_CODE_INVALID_PAYLOAD,
        RPC_ERROR_CODE_INVALID_REQUEST_ID, RPC_ERROR_CODE_UNSUPPORTED_KIND,
        RPC_ERROR_CODE_UNSUPPORTED_SCHEMA, RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY,
    };

    const RPC_SCHEMA_COMPAT_FIXTURE_SCHEMA_VERSION: u32 = 1;

    #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum RpcSchemaCompatMode {
        DispatchNdjson,
        ServeNdjson,
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct RpcSchemaCompatFixture {
        schema_version: u32,
        name: String,
        mode: RpcSchemaCompatMode,
        input_lines: Vec<String>,
        expected_processed_lines: usize,
        expected_error_count: usize,
        expected_responses: Vec<Value>,
    }

    fn rpc_schema_compat_fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("rpc-schema-compat")
            .join(name)
    }

    fn parse_rpc_schema_compat_fixture(raw: &str) -> Result<RpcSchemaCompatFixture> {
        let fixture = serde_json::from_str::<RpcSchemaCompatFixture>(raw)
            .context("failed to parse rpc schema compatibility fixture")?;
        validate_rpc_schema_compat_fixture(&fixture)?;
        Ok(fixture)
    }

    fn load_rpc_schema_compat_fixture(name: &str) -> RpcSchemaCompatFixture {
        let path = rpc_schema_compat_fixture_path(name);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        parse_rpc_schema_compat_fixture(&raw)
            .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", path.display()))
    }

    fn validate_rpc_schema_compat_fixture(fixture: &RpcSchemaCompatFixture) -> Result<()> {
        if fixture.schema_version != RPC_SCHEMA_COMPAT_FIXTURE_SCHEMA_VERSION {
            bail!(
                "unsupported rpc schema compatibility fixture schema_version {} (expected {})",
                fixture.schema_version,
                RPC_SCHEMA_COMPAT_FIXTURE_SCHEMA_VERSION
            );
        }
        if fixture.name.trim().is_empty() {
            bail!("rpc schema compatibility fixture name cannot be empty");
        }
        if fixture.input_lines.is_empty() {
            bail!(
                "rpc schema compatibility fixture '{}' must include at least one input line",
                fixture.name
            );
        }
        if fixture.expected_responses.is_empty() {
            bail!(
                "rpc schema compatibility fixture '{}' must include at least one expected response",
                fixture.name
            );
        }
        Ok(())
    }

    fn replay_rpc_schema_compat_fixture(
        fixture: &RpcSchemaCompatFixture,
    ) -> (usize, usize, Vec<Value>) {
        let input = fixture.input_lines.join("\n");
        match fixture.mode {
            RpcSchemaCompatMode::DispatchNdjson => {
                let report = dispatch_rpc_ndjson_input(&input);
                (
                    report.processed_lines,
                    report.error_count,
                    report
                        .responses
                        .into_iter()
                        .map(normalize_rpc_response_frame)
                        .collect::<Vec<_>>(),
                )
            }
            RpcSchemaCompatMode::ServeNdjson => {
                let mut output = Vec::new();
                let report = serve_rpc_ndjson_reader(Cursor::new(input), &mut output)
                    .expect("serve fixture replay should succeed");
                let raw = String::from_utf8(output).expect("fixture output should be utf8");
                (
                    report.processed_lines,
                    report.error_count,
                    parse_rpc_ndjson_lines(&raw),
                )
            }
        }
    }

    fn normalize_rpc_response_frame(frame: RpcResponseFrame) -> Value {
        serde_json::to_value(frame).expect("rpc response frame should serialize")
    }

    fn parse_rpc_ndjson_lines(raw: &str) -> Vec<Value> {
        raw.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<Value>(line).expect("line should parse as json"))
            .collect::<Vec<_>>()
    }

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

        let legacy_frame = parse_rpc_frame(
            r#"{
  "schema_version": 0,
  "request_id": "req-legacy",
  "kind": "run.cancel",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse legacy frame");
        assert_eq!(legacy_frame.request_id, "req-legacy");
        assert_eq!(legacy_frame.kind, RpcFrameKind::RunCancel);

        let status_frame = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-status",
  "kind": "run.status",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse status frame");
        assert_eq!(status_frame.kind, RpcFrameKind::RunStatus);

        let complete_frame = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-complete",
  "kind": "run.complete",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse complete frame");
        assert_eq!(complete_frame.kind, RpcFrameKind::RunComplete);

        let fail_frame = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-fail",
  "kind": "run.fail",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse fail frame");
        assert_eq!(fail_frame.kind, RpcFrameKind::RunFail);

        let timeout_frame = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-timeout",
  "kind": "run.timeout",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse timeout frame");
        assert_eq!(timeout_frame.kind, RpcFrameKind::RunTimeout);
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
        assert_eq!(
            capabilities_response.payload["response_schema_version"].as_u64(),
            Some(1)
        );
        let schema_versions = capabilities_response.payload["supported_request_schema_versions"]
            .as_array()
            .expect("supported schemas array");
        assert_eq!(schema_versions.len(), 2);
        assert_eq!(schema_versions[0].as_u64(), Some(0));
        assert_eq!(schema_versions[1].as_u64(), Some(1));

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
        assert_eq!(
            start_response.payload["run_id"].as_str(),
            Some("run-req-start")
        );

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
        assert_eq!(cancel_response.payload["terminal"].as_bool(), Some(true));
        assert_eq!(
            cancel_response.payload["terminal_state"].as_str(),
            Some("cancelled")
        );

        let complete = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-complete",
  "kind": "run.complete",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse complete");
        let complete_response = dispatch_rpc_frame(&complete).expect("dispatch complete");
        assert_eq!(complete_response.kind, "run.completed");
        assert_eq!(complete_response.payload["run_id"].as_str(), Some("run-1"));
        assert_eq!(complete_response.payload["terminal"].as_bool(), Some(true));
        assert_eq!(
            complete_response.payload["terminal_state"].as_str(),
            Some("completed")
        );

        let fail = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-fail",
  "kind": "run.fail",
  "payload": {"run_id":"run-1","reason":"tool failure"}
}"#,
        )
        .expect("parse fail");
        let fail_response = dispatch_rpc_frame(&fail).expect("dispatch fail");
        assert_eq!(fail_response.kind, "run.failed");
        assert_eq!(fail_response.payload["run_id"].as_str(), Some("run-1"));
        assert_eq!(
            fail_response.payload["reason"].as_str(),
            Some("tool failure")
        );
        assert_eq!(fail_response.payload["terminal"].as_bool(), Some(true));
        assert_eq!(
            fail_response.payload["terminal_state"].as_str(),
            Some("failed")
        );

        let timeout = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-timeout",
  "kind": "run.timeout",
  "payload": {"run_id":"run-1","reason":"request timeout"}
}"#,
        )
        .expect("parse timeout");
        let timeout_response = dispatch_rpc_frame(&timeout).expect("dispatch timeout");
        assert_eq!(timeout_response.kind, "run.timed_out");
        assert_eq!(timeout_response.payload["run_id"].as_str(), Some("run-1"));
        assert_eq!(
            timeout_response.payload["reason"].as_str(),
            Some("request timeout")
        );
        assert_eq!(timeout_response.payload["terminal"].as_bool(), Some(true));
        assert_eq!(
            timeout_response.payload["terminal_state"].as_str(),
            Some("timed_out")
        );

        let status = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-status",
  "kind": "run.status",
  "payload": {"run_id":"run-1"}
}"#,
        )
        .expect("parse status");
        let status_response = dispatch_rpc_frame(&status).expect("dispatch status");
        assert_eq!(status_response.kind, "run.status");
        assert_eq!(status_response.payload["run_id"].as_str(), Some("run-1"));
        assert_eq!(status_response.payload["active"].as_bool(), Some(false));
        assert_eq!(status_response.payload["known"].as_bool(), Some(false));
        assert_eq!(status_response.payload["terminal"].as_bool(), Some(false));
    }

    #[test]
    fn functional_dispatch_rpc_frame_accepts_legacy_schema_zero() {
        let frame = parse_rpc_frame(
            r#"{
  "schema_version": 0,
  "request_id": "req-legacy-start",
  "kind": "run.start",
  "payload": {"prompt":"legacy hello"}
}"#,
        )
        .expect("parse frame");
        let response = dispatch_rpc_frame(&frame).expect("dispatch frame");
        assert_eq!(response.kind, "run.accepted");
        assert_eq!(response.request_id, "req-legacy-start");
        assert_eq!(
            response.payload["run_id"].as_str(),
            Some("run-req-legacy-start")
        );
    }

    #[test]
    fn unit_dispatch_rpc_frame_run_start_accepts_explicit_run_id() {
        let frame = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-start-explicit",
  "kind": "run.start",
  "payload": {"prompt":"hello world","run_id":"my-run"}
}"#,
        )
        .expect("parse frame");
        let response = dispatch_rpc_frame(&frame).expect("dispatch frame");
        assert_eq!(response.kind, "run.accepted");
        assert_eq!(response.payload["run_id"].as_str(), Some("my-run"));
    }

    #[test]
    fn unit_dispatch_rpc_frame_for_serve_terminal_assistant_frame_shape_is_stable() {
        let mut state = RpcServeSessionState::default();
        let start = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {"prompt":"hello","run_id":"run-terminal"}
}"#,
        )
        .expect("parse start");
        let start_responses =
            dispatch_rpc_frame_for_serve(&start, &mut state).expect("dispatch start");
        assert_eq!(start_responses.len(), 3);

        let complete = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-complete",
  "kind": "run.complete",
  "payload": {"run_id":"run-terminal"}
}"#,
        )
        .expect("parse complete");
        let complete_responses =
            dispatch_rpc_frame_for_serve(&complete, &mut state).expect("dispatch complete");
        assert_eq!(complete_responses.len(), 3);
        assert_eq!(complete_responses[2].kind, "run.stream.assistant_text");
        assert_eq!(complete_responses[2].payload["run_id"], "run-terminal");
        assert_eq!(complete_responses[2].payload["delta"], "run completed");
        assert_eq!(complete_responses[2].payload["final"], true);
        assert_eq!(complete_responses[2].payload["terminal"], true);
        assert_eq!(complete_responses[2].payload["terminal_state"], "completed");
        assert_eq!(complete_responses[2].payload["sequence"], 3);
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

        let complete = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-complete",
  "kind": "run.complete",
  "payload": {}
}"#,
        )
        .expect("parse complete");
        let complete_error = dispatch_rpc_frame(&complete).expect_err("missing run_id should fail");
        assert!(complete_error
            .to_string()
            .contains("requires non-empty payload field 'run_id'"));

        let fail = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-fail",
  "kind": "run.fail",
  "payload": {}
}"#,
        )
        .expect("parse fail");
        let fail_error = dispatch_rpc_frame(&fail).expect_err("missing run_id should fail");
        assert!(fail_error
            .to_string()
            .contains("requires non-empty payload field 'run_id'"));

        let fail_invalid_reason = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-fail-invalid-reason",
  "kind": "run.fail",
  "payload": {"run_id":"run-1","reason":""}
}"#,
        )
        .expect("parse invalid fail reason");
        let fail_invalid_reason_error =
            dispatch_rpc_frame(&fail_invalid_reason).expect_err("invalid reason should fail");
        assert!(fail_invalid_reason_error
            .to_string()
            .contains("optional payload field 'reason'"));

        let timeout = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-timeout",
  "kind": "run.timeout",
  "payload": {}
}"#,
        )
        .expect("parse timeout");
        let timeout_error = dispatch_rpc_frame(&timeout).expect_err("missing run_id should fail");
        assert!(timeout_error
            .to_string()
            .contains("requires non-empty payload field 'run_id'"));

        let timeout_invalid_reason = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-timeout-invalid-reason",
  "kind": "run.timeout",
  "payload": {"run_id":"run-1","reason":""}
}"#,
        )
        .expect("parse invalid timeout reason");
        let timeout_invalid_reason_error =
            dispatch_rpc_frame(&timeout_invalid_reason).expect_err("invalid reason should fail");
        assert!(timeout_invalid_reason_error
            .to_string()
            .contains("optional payload field 'reason'"));

        let status = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-status",
  "kind": "run.status",
  "payload": {}
}"#,
        )
        .expect("parse status");
        let status_error = dispatch_rpc_frame(&status).expect_err("missing run_id should fail");
        assert!(status_error
            .to_string()
            .contains("requires non-empty payload field 'run_id'"));

        let start_invalid_run_id = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-start-invalid",
  "kind": "run.start",
  "payload": {"prompt":"x","run_id":""}
}"#,
        )
        .expect("parse invalid run_id frame");
        let start_invalid_run_id_error =
            dispatch_rpc_frame(&start_invalid_run_id).expect_err("empty run_id should fail");
        assert!(start_invalid_run_id_error
            .to_string()
            .contains("optional payload field 'run_id'"));
    }

    #[test]
    fn unit_classify_rpc_error_message_maps_known_validation_classes() {
        assert_eq!(
            classify_rpc_error_message("failed to parse rpc frame JSON: expected value"),
            RPC_ERROR_CODE_INVALID_JSON
        );
        assert_eq!(
            classify_rpc_error_message(
                "unsupported rpc frame schema: supported request schema versions are [0, 1], found 2"
            ),
            RPC_ERROR_CODE_UNSUPPORTED_SCHEMA
        );
        assert_eq!(
            classify_rpc_error_message(
                "unsupported rpc frame kind 'x'; supported kinds are capabilities.request, run.start, run.cancel, run.complete, run.fail, run.timeout, run.status"
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
{"schema_version":1,"request_id":"req-status","kind":"run.status","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-complete","kind":"run.complete","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-fail","kind":"run.fail","payload":{"run_id":"run-req-start","reason":"failed for testing"}}
{"schema_version":1,"request_id":"req-timeout","kind":"run.timeout","payload":{"run_id":"run-req-start","reason":"timeout in dispatch"}}
"#,
        );
        assert_eq!(report.processed_lines, 6);
        assert_eq!(report.error_count, 0);
        assert_eq!(report.responses.len(), 6);
        assert_eq!(report.responses[0].request_id, "req-cap");
        assert_eq!(report.responses[0].kind, "capabilities.response");
        assert_eq!(report.responses[1].request_id, "req-start");
        assert_eq!(report.responses[1].kind, "run.accepted");
        assert_eq!(report.responses[2].request_id, "req-status");
        assert_eq!(report.responses[2].kind, "run.status");
        assert_eq!(report.responses[2].payload["active"].as_bool(), Some(false));
        assert_eq!(
            report.responses[2].payload["terminal"].as_bool(),
            Some(false)
        );
        assert_eq!(report.responses[3].request_id, "req-complete");
        assert_eq!(report.responses[3].kind, "run.completed");
        assert_eq!(
            report.responses[3].payload["terminal"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report.responses[3].payload["terminal_state"].as_str(),
            Some("completed")
        );
        assert_eq!(report.responses[4].request_id, "req-fail");
        assert_eq!(report.responses[4].kind, "run.failed");
        assert_eq!(
            report.responses[4].payload["reason"].as_str(),
            Some("failed for testing")
        );
        assert_eq!(
            report.responses[4].payload["terminal"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report.responses[4].payload["terminal_state"].as_str(),
            Some("failed")
        );
        assert_eq!(report.responses[5].request_id, "req-timeout");
        assert_eq!(report.responses[5].kind, "run.timed_out");
        assert_eq!(
            report.responses[5].payload["reason"].as_str(),
            Some("timeout in dispatch")
        );
        assert_eq!(
            report.responses[5].payload["terminal"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report.responses[5].payload["terminal_state"].as_str(),
            Some("timed_out")
        );
    }

    #[test]
    fn integration_dispatch_rpc_ndjson_input_supports_mixed_schema_versions() {
        let report = dispatch_rpc_ndjson_input(
            r#"
{"schema_version":0,"request_id":"req-legacy","kind":"run.cancel","payload":{"run_id":"run-1"}}
{"schema_version":1,"request_id":"req-current","kind":"run.start","payload":{"prompt":"hello"}}
"#,
        );
        assert_eq!(report.processed_lines, 2);
        assert_eq!(report.error_count, 0);
        assert_eq!(report.responses.len(), 2);
        assert_eq!(report.responses[0].kind, "run.cancelled");
        assert_eq!(
            report.responses[0].payload["terminal"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report.responses[0].payload["terminal_state"].as_str(),
            Some("cancelled")
        );
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

    #[test]
    fn unit_serve_rpc_ndjson_reader_skips_blank_and_comment_lines() {
        let input = r#"
# comment

{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 1);
        assert_eq!(report.error_count, 0);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines.lines().collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        let response: serde_json::Value = serde_json::from_str(rows[0]).expect("json frame");
        assert_eq!(response["request_id"], "req-cap");
        assert_eq!(response["kind"], "capabilities.response");
    }

    #[test]
    fn functional_serve_rpc_ndjson_reader_emits_ordered_responses_for_mixed_frames() {
        let input = r#"
{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-status-active","kind":"run.status","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-cancel","kind":"run.cancel","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-status-inactive","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 5);
        assert_eq!(report.error_count, 0);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0]["request_id"], "req-cap");
        assert_eq!(rows[0]["kind"], "capabilities.response");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.accepted");
        assert_eq!(rows[1]["payload"]["run_id"], "run-req-start");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.tool_events");
        assert_eq!(rows[2]["payload"]["run_id"], "run-req-start");
        assert_eq!(rows[2]["payload"]["event"], "run.started");
        assert_eq!(rows[3]["request_id"], "req-start");
        assert_eq!(rows[3]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[3]["payload"]["run_id"], "run-req-start");
        assert_eq!(
            rows[3]["payload"]["delta"],
            "preflight run accepted (5 prompt chars)"
        );
        assert_eq!(rows[4]["request_id"], "req-status-active");
        assert_eq!(rows[4]["kind"], "run.status");
        assert_eq!(rows[4]["payload"]["active"], true);
        assert_eq!(rows[4]["payload"]["terminal"], false);
        assert_eq!(rows[5]["request_id"], "req-cancel");
        assert_eq!(rows[5]["kind"], "run.cancelled");
        assert_eq!(rows[5]["payload"]["terminal"], true);
        assert_eq!(rows[5]["payload"]["terminal_state"], "cancelled");
        assert_eq!(rows[6]["request_id"], "req-cancel");
        assert_eq!(rows[6]["kind"], "run.stream.tool_events");
        assert_eq!(rows[6]["payload"]["event"], "run.cancelled");
        assert_eq!(rows[6]["payload"]["terminal"], true);
        assert_eq!(rows[6]["payload"]["terminal_state"], "cancelled");
        assert_eq!(rows[7]["request_id"], "req-cancel");
        assert_eq!(rows[7]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[7]["payload"]["run_id"], "run-req-start");
        assert_eq!(rows[7]["payload"]["delta"], "run cancelled");
        assert_eq!(rows[7]["payload"]["final"], true);
        assert_eq!(rows[7]["payload"]["terminal"], true);
        assert_eq!(rows[7]["payload"]["terminal_state"], "cancelled");
        assert_eq!(rows[7]["payload"]["sequence"], 3);
        assert_eq!(rows[8]["request_id"], "req-status-inactive");
        assert_eq!(rows[8]["kind"], "run.status");
        assert_eq!(rows[8]["payload"]["active"], false);
        assert_eq!(rows[8]["payload"]["known"], true);
        assert_eq!(rows[8]["payload"]["status"], "cancelled");
        assert_eq!(rows[8]["payload"]["terminal"], true);
        assert_eq!(rows[8]["payload"]["terminal_state"], "cancelled");
    }

    #[test]
    fn functional_serve_rpc_ndjson_reader_supports_run_complete_lifecycle_transition() {
        let input = r#"
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-status-active","kind":"run.status","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-complete","kind":"run.complete","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-status-inactive","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 4);
        assert_eq!(report.error_count, 0);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0]["request_id"], "req-start");
        assert_eq!(rows[0]["kind"], "run.accepted");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.stream.tool_events");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[3]["request_id"], "req-status-active");
        assert_eq!(rows[3]["kind"], "run.status");
        assert_eq!(rows[3]["payload"]["active"], true);
        assert_eq!(rows[3]["payload"]["terminal"], false);
        assert_eq!(rows[4]["request_id"], "req-complete");
        assert_eq!(rows[4]["kind"], "run.completed");
        assert_eq!(rows[4]["payload"]["terminal"], true);
        assert_eq!(rows[4]["payload"]["terminal_state"], "completed");
        assert_eq!(rows[5]["request_id"], "req-complete");
        assert_eq!(rows[5]["kind"], "run.stream.tool_events");
        assert_eq!(rows[5]["payload"]["event"], "run.completed");
        assert_eq!(rows[5]["payload"]["terminal"], true);
        assert_eq!(rows[5]["payload"]["terminal_state"], "completed");
        assert_eq!(rows[6]["request_id"], "req-complete");
        assert_eq!(rows[6]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[6]["payload"]["run_id"], "run-req-start");
        assert_eq!(rows[6]["payload"]["delta"], "run completed");
        assert_eq!(rows[6]["payload"]["final"], true);
        assert_eq!(rows[6]["payload"]["terminal"], true);
        assert_eq!(rows[6]["payload"]["terminal_state"], "completed");
        assert_eq!(rows[6]["payload"]["sequence"], 3);
        assert_eq!(rows[7]["request_id"], "req-status-inactive");
        assert_eq!(rows[7]["kind"], "run.status");
        assert_eq!(rows[7]["payload"]["active"], false);
        assert_eq!(rows[7]["payload"]["known"], true);
        assert_eq!(rows[7]["payload"]["status"], "completed");
        assert_eq!(rows[7]["payload"]["terminal"], true);
        assert_eq!(rows[7]["payload"]["terminal_state"], "completed");
    }

    #[test]
    fn functional_serve_rpc_ndjson_reader_supports_run_fail_lifecycle_transition() {
        let input = r#"
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-status-active","kind":"run.status","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-fail","kind":"run.fail","payload":{"run_id":"run-req-start","reason":"provider timeout"}}
{"schema_version":1,"request_id":"req-status-inactive","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 4);
        assert_eq!(report.error_count, 0);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0]["request_id"], "req-start");
        assert_eq!(rows[0]["kind"], "run.accepted");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.stream.tool_events");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[3]["request_id"], "req-status-active");
        assert_eq!(rows[3]["kind"], "run.status");
        assert_eq!(rows[3]["payload"]["active"], true);
        assert_eq!(rows[3]["payload"]["terminal"], false);
        assert_eq!(rows[4]["request_id"], "req-fail");
        assert_eq!(rows[4]["kind"], "run.failed");
        assert_eq!(rows[4]["payload"]["reason"], "provider timeout");
        assert_eq!(rows[4]["payload"]["terminal"], true);
        assert_eq!(rows[4]["payload"]["terminal_state"], "failed");
        assert_eq!(rows[5]["request_id"], "req-fail");
        assert_eq!(rows[5]["kind"], "run.stream.tool_events");
        assert_eq!(rows[5]["payload"]["event"], "run.failed");
        assert_eq!(rows[5]["payload"]["reason"], "provider timeout");
        assert_eq!(rows[5]["payload"]["terminal"], true);
        assert_eq!(rows[5]["payload"]["terminal_state"], "failed");
        assert_eq!(rows[6]["request_id"], "req-fail");
        assert_eq!(rows[6]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[6]["payload"]["run_id"], "run-req-start");
        assert_eq!(rows[6]["payload"]["delta"], "run failed: provider timeout");
        assert_eq!(rows[6]["payload"]["final"], true);
        assert_eq!(rows[6]["payload"]["terminal"], true);
        assert_eq!(rows[6]["payload"]["terminal_state"], "failed");
        assert_eq!(rows[6]["payload"]["reason"], "provider timeout");
        assert_eq!(rows[6]["payload"]["sequence"], 3);
        assert_eq!(rows[7]["request_id"], "req-status-inactive");
        assert_eq!(rows[7]["kind"], "run.status");
        assert_eq!(rows[7]["payload"]["active"], false);
        assert_eq!(rows[7]["payload"]["known"], true);
        assert_eq!(rows[7]["payload"]["status"], "failed");
        assert_eq!(rows[7]["payload"]["terminal"], true);
        assert_eq!(rows[7]["payload"]["terminal_state"], "failed");
        assert_eq!(rows[7]["payload"]["reason"], "provider timeout");
    }

    #[test]
    fn functional_serve_rpc_ndjson_reader_supports_run_timeout_lifecycle_transition() {
        let input = r#"
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-status-active","kind":"run.status","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-timeout","kind":"run.timeout","payload":{"run_id":"run-req-start","reason":"client timeout"}}
{"schema_version":1,"request_id":"req-status-inactive","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 4);
        assert_eq!(report.error_count, 0);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0]["request_id"], "req-start");
        assert_eq!(rows[0]["kind"], "run.accepted");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.stream.tool_events");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[3]["request_id"], "req-status-active");
        assert_eq!(rows[3]["kind"], "run.status");
        assert_eq!(rows[3]["payload"]["active"], true);
        assert_eq!(rows[3]["payload"]["terminal"], false);
        assert_eq!(rows[4]["request_id"], "req-timeout");
        assert_eq!(rows[4]["kind"], "run.timed_out");
        assert_eq!(rows[4]["payload"]["reason"], "client timeout");
        assert_eq!(rows[4]["payload"]["terminal"], true);
        assert_eq!(rows[4]["payload"]["terminal_state"], "timed_out");
        assert_eq!(rows[5]["request_id"], "req-timeout");
        assert_eq!(rows[5]["kind"], "run.stream.tool_events");
        assert_eq!(rows[5]["payload"]["event"], "run.timed_out");
        assert_eq!(rows[5]["payload"]["reason"], "client timeout");
        assert_eq!(rows[5]["payload"]["terminal"], true);
        assert_eq!(rows[5]["payload"]["terminal_state"], "timed_out");
        assert_eq!(rows[6]["request_id"], "req-timeout");
        assert_eq!(rows[6]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[6]["payload"]["run_id"], "run-req-start");
        assert_eq!(rows[6]["payload"]["delta"], "run timed out: client timeout");
        assert_eq!(rows[6]["payload"]["final"], true);
        assert_eq!(rows[6]["payload"]["terminal"], true);
        assert_eq!(rows[6]["payload"]["terminal_state"], "timed_out");
        assert_eq!(rows[6]["payload"]["reason"], "client timeout");
        assert_eq!(rows[6]["payload"]["sequence"], 3);
        assert_eq!(rows[7]["request_id"], "req-status-inactive");
        assert_eq!(rows[7]["kind"], "run.status");
        assert_eq!(rows[7]["payload"]["active"], false);
        assert_eq!(rows[7]["payload"]["known"], true);
        assert_eq!(rows[7]["payload"]["status"], "timed_out");
        assert_eq!(rows[7]["payload"]["terminal"], true);
        assert_eq!(rows[7]["payload"]["terminal_state"], "timed_out");
        assert_eq!(rows[7]["payload"]["reason"], "client timeout");
    }

    #[test]
    fn functional_serve_rpc_ndjson_reader_reuses_run_id_and_clears_closed_status_history() {
        let input = r#"
{"schema_version":1,"request_id":"req-start-1","kind":"run.start","payload":{"prompt":"hello","run_id":"run-fixed"}}
{"schema_version":1,"request_id":"req-complete","kind":"run.complete","payload":{"run_id":"run-fixed"}}
{"schema_version":1,"request_id":"req-status-closed","kind":"run.status","payload":{"run_id":"run-fixed"}}
{"schema_version":1,"request_id":"req-start-2","kind":"run.start","payload":{"prompt":"hello again","run_id":"run-fixed"}}
{"schema_version":1,"request_id":"req-status-active","kind":"run.status","payload":{"run_id":"run-fixed"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 5);
        assert_eq!(report.error_count, 0);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 11);
        assert_eq!(rows[0]["request_id"], "req-start-1");
        assert_eq!(rows[0]["kind"], "run.accepted");
        assert_eq!(rows[0]["payload"]["run_id"], "run-fixed");
        assert_eq!(rows[3]["request_id"], "req-complete");
        assert_eq!(rows[3]["kind"], "run.completed");
        assert_eq!(rows[4]["request_id"], "req-complete");
        assert_eq!(rows[4]["kind"], "run.stream.tool_events");
        assert_eq!(rows[5]["request_id"], "req-complete");
        assert_eq!(rows[5]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[5]["payload"]["run_id"], "run-fixed");
        assert_eq!(rows[5]["payload"]["final"], true);
        assert_eq!(rows[5]["payload"]["terminal_state"], "completed");
        assert_eq!(rows[6]["request_id"], "req-status-closed");
        assert_eq!(rows[6]["kind"], "run.status");
        assert_eq!(rows[6]["payload"]["active"], false);
        assert_eq!(rows[6]["payload"]["known"], true);
        assert_eq!(rows[6]["payload"]["status"], "completed");
        assert_eq!(rows[7]["request_id"], "req-start-2");
        assert_eq!(rows[7]["kind"], "run.accepted");
        assert_eq!(rows[7]["payload"]["run_id"], "run-fixed");
        assert_eq!(rows[10]["request_id"], "req-status-active");
        assert_eq!(rows[10]["kind"], "run.status");
        assert_eq!(rows[10]["payload"]["active"], true);
        assert_eq!(rows[10]["payload"]["known"], true);
        assert_eq!(rows[10]["payload"]["status"], "active");
        assert_eq!(rows[10]["payload"].get("terminal_state"), None);
        assert_eq!(rows[10]["payload"]["terminal"], false);
    }

    #[test]
    fn regression_serve_rpc_closed_status_memory_evicts_oldest_entries_at_capacity() {
        let mut state = RpcServeSessionState::default();

        for index in 0..=RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY {
            let run_id = format!("run-{index}");
            let start = parse_rpc_frame(&format!(
                r#"{{"schema_version":1,"request_id":"req-start-{index}","kind":"run.start","payload":{{"prompt":"x","run_id":"{run_id}"}}}}"#
            ))
            .expect("parse start");
            dispatch_rpc_frame_for_serve(&start, &mut state).expect("dispatch start");

            let complete = parse_rpc_frame(&format!(
                r#"{{"schema_version":1,"request_id":"req-complete-{index}","kind":"run.complete","payload":{{"run_id":"{run_id}"}}}}"#
            ))
            .expect("parse complete");
            dispatch_rpc_frame_for_serve(&complete, &mut state).expect("dispatch complete");
        }

        assert_eq!(
            state.closed_run_states.len(),
            RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY
        );
        assert_eq!(
            state.closed_run_order.len(),
            RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY
        );
        assert!(!state.closed_run_states.contains_key("run-0"));
        assert!(state
            .closed_run_states
            .contains_key(&format!("run-{}", RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY)));

        let oldest_status = parse_rpc_frame(
            r#"{
  "schema_version": 1,
  "request_id": "req-status-oldest",
  "kind": "run.status",
  "payload": {"run_id":"run-0"}
}"#,
        )
        .expect("parse oldest status");
        let oldest = dispatch_rpc_frame_for_serve(&oldest_status, &mut state)
            .expect("dispatch oldest status");
        assert_eq!(oldest.len(), 1);
        assert_eq!(oldest[0].kind, "run.status");
        assert_eq!(oldest[0].payload["known"], false);
        assert_eq!(oldest[0].payload["terminal"], false);

        let newest_id = format!("run-{}", RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY);
        let newest_status = parse_rpc_frame(&format!(
            r#"{{"schema_version":1,"request_id":"req-status-newest","kind":"run.status","payload":{{"run_id":"{newest_id}"}}}}"#
        ))
        .expect("parse newest status");
        let newest = dispatch_rpc_frame_for_serve(&newest_status, &mut state)
            .expect("dispatch newest status");
        assert_eq!(newest.len(), 1);
        assert_eq!(newest[0].kind, "run.status");
        assert_eq!(newest[0].payload["known"], true);
        assert_eq!(newest[0].payload["status"], "completed");
        assert_eq!(newest[0].payload["terminal"], true);
        assert_eq!(newest[0].payload["terminal_state"], "completed");
    }

    #[test]
    fn regression_serve_rpc_ndjson_reader_keeps_processing_after_malformed_json() {
        let input = r#"
{"schema_version":1,"request_id":"req-ok","kind":"run.start","payload":{"prompt":"x"}}
not-json
{"schema_version":1,"request_id":"req-ok-2","kind":"run.cancel","payload":{"run_id":"run-req-ok"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 3);
        assert_eq!(report.error_count, 1);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0]["request_id"], "req-ok");
        assert_eq!(rows[0]["kind"], "run.accepted");
        assert_eq!(rows[0]["payload"]["run_id"], "run-req-ok");
        assert_eq!(rows[1]["request_id"], "req-ok");
        assert_eq!(rows[1]["kind"], "run.stream.tool_events");
        assert_eq!(rows[2]["request_id"], "req-ok");
        assert_eq!(rows[2]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[3]["kind"], "error");
        assert_eq!(rows[3]["payload"]["code"], "invalid_json");
        assert_eq!(rows[4]["request_id"], "req-ok-2");
        assert_eq!(rows[4]["kind"], "run.cancelled");
        assert_eq!(rows[5]["request_id"], "req-ok-2");
        assert_eq!(rows[5]["kind"], "run.stream.tool_events");
        assert_eq!(rows[5]["payload"]["event"], "run.cancelled");
        assert_eq!(rows[6]["request_id"], "req-ok-2");
        assert_eq!(rows[6]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[6]["payload"]["run_id"], "run-req-ok");
        assert_eq!(rows[6]["payload"]["delta"], "run cancelled");
        assert_eq!(rows[6]["payload"]["final"], true);
        assert_eq!(rows[6]["payload"]["terminal_state"], "cancelled");
    }

    #[test]
    fn regression_serve_rpc_ndjson_reader_rejects_unknown_run_cancel_and_continues() {
        let input = r#"
{"schema_version":1,"request_id":"req-bad-cancel","kind":"run.cancel","payload":{"run_id":"run-missing"}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"x"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 2);
        assert_eq!(report.error_count, 1);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["request_id"], "req-bad-cancel");
        assert_eq!(rows[0]["kind"], "error");
        assert_eq!(rows[0]["payload"]["code"], "invalid_payload");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.accepted");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.tool_events");
        assert_eq!(rows[3]["request_id"], "req-start");
        assert_eq!(rows[3]["kind"], "run.stream.assistant_text");
    }

    #[test]
    fn regression_serve_rpc_ndjson_reader_rejects_unknown_run_complete_and_continues() {
        let input = r#"
{"schema_version":1,"request_id":"req-bad-complete","kind":"run.complete","payload":{"run_id":"run-missing"}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"x"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 2);
        assert_eq!(report.error_count, 1);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["request_id"], "req-bad-complete");
        assert_eq!(rows[0]["kind"], "error");
        assert_eq!(rows[0]["payload"]["code"], "invalid_payload");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.accepted");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.tool_events");
        assert_eq!(rows[3]["request_id"], "req-start");
        assert_eq!(rows[3]["kind"], "run.stream.assistant_text");
    }

    #[test]
    fn regression_serve_rpc_ndjson_reader_rejects_unknown_run_fail_and_continues() {
        let input = r#"
{"schema_version":1,"request_id":"req-bad-fail","kind":"run.fail","payload":{"run_id":"run-missing","reason":"oops"}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"x"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 2);
        assert_eq!(report.error_count, 1);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["request_id"], "req-bad-fail");
        assert_eq!(rows[0]["kind"], "error");
        assert_eq!(rows[0]["payload"]["code"], "invalid_payload");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.accepted");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.tool_events");
        assert_eq!(rows[3]["request_id"], "req-start");
        assert_eq!(rows[3]["kind"], "run.stream.assistant_text");
    }

    #[test]
    fn regression_serve_rpc_ndjson_reader_rejects_unknown_run_timeout_and_continues() {
        let input = r#"
{"schema_version":1,"request_id":"req-bad-timeout","kind":"run.timeout","payload":{"run_id":"run-missing","reason":"timeout"}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"x"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 2);
        assert_eq!(report.error_count, 1);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["request_id"], "req-bad-timeout");
        assert_eq!(rows[0]["kind"], "error");
        assert_eq!(rows[0]["payload"]["code"], "invalid_payload");
        assert_eq!(rows[1]["request_id"], "req-start");
        assert_eq!(rows[1]["kind"], "run.accepted");
        assert_eq!(rows[2]["request_id"], "req-start");
        assert_eq!(rows[2]["kind"], "run.stream.tool_events");
        assert_eq!(rows[3]["request_id"], "req-start");
        assert_eq!(rows[3]["kind"], "run.stream.assistant_text");
    }

    #[test]
    fn regression_serve_rpc_ndjson_reader_rejects_duplicate_active_run_ids() {
        let input = r#"
{"schema_version":1,"request_id":"req-start-1","kind":"run.start","payload":{"prompt":"x","run_id":"run-shared"}}
{"schema_version":1,"request_id":"req-start-2","kind":"run.start","payload":{"prompt":"x","run_id":"run-shared"}}
"#;
        let mut output = Vec::new();
        let report = serve_rpc_ndjson_reader(std::io::Cursor::new(input), &mut output)
            .expect("serve should succeed");
        assert_eq!(report.processed_lines, 2);
        assert_eq!(report.error_count, 1);

        let lines = String::from_utf8(output).expect("utf8 output");
        let rows = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json frame"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["request_id"], "req-start-1");
        assert_eq!(rows[0]["kind"], "run.accepted");
        assert_eq!(rows[1]["request_id"], "req-start-1");
        assert_eq!(rows[1]["kind"], "run.stream.tool_events");
        assert_eq!(rows[2]["request_id"], "req-start-1");
        assert_eq!(rows[2]["kind"], "run.stream.assistant_text");
        assert_eq!(rows[3]["request_id"], "req-start-2");
        assert_eq!(rows[3]["kind"], "error");
        assert_eq!(rows[3]["payload"]["code"], "invalid_payload");
    }

    #[test]
    fn unit_parse_rpc_schema_compat_fixture_rejects_unsupported_fixture_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "invalid",
  "mode": "dispatch_ndjson",
  "input_lines": ["{\"schema_version\":1,\"request_id\":\"req\",\"kind\":\"capabilities.request\",\"payload\":{}}"],
  "expected_processed_lines": 1,
  "expected_error_count": 0,
  "expected_responses": [{"schema_version":1,"request_id":"req","kind":"capabilities.response","payload":{"capabilities":[]}}]
}"#;

        let error = parse_rpc_schema_compat_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported rpc schema compatibility fixture schema_version"));
    }

    #[test]
    fn functional_rpc_schema_compat_dispatch_fixture_replays_supported_versions() {
        let fixture = load_rpc_schema_compat_fixture("dispatch-mixed-supported.json");
        let (processed_lines, error_count, responses) = replay_rpc_schema_compat_fixture(&fixture);
        assert_eq!(processed_lines, fixture.expected_processed_lines);
        assert_eq!(error_count, fixture.expected_error_count);
        assert_eq!(responses, fixture.expected_responses);
    }

    #[test]
    fn functional_rpc_schema_compat_serve_fixture_replays_supported_versions() {
        for name in ["serve-mixed-supported.json", "serve-cancel-supported.json"] {
            let fixture = load_rpc_schema_compat_fixture(name);
            let (processed_lines, error_count, responses) =
                replay_rpc_schema_compat_fixture(&fixture);
            assert_eq!(processed_lines, fixture.expected_processed_lines);
            assert_eq!(error_count, fixture.expected_error_count);
            assert_eq!(responses, fixture.expected_responses);
        }
    }

    #[test]
    fn integration_rpc_schema_compat_fixture_replay_is_deterministic_across_modes() {
        for name in [
            "dispatch-mixed-supported.json",
            "dispatch-unsupported-continues.json",
            "serve-mixed-supported.json",
            "serve-cancel-supported.json",
            "serve-unsupported-continues.json",
        ] {
            let fixture = load_rpc_schema_compat_fixture(name);
            let first = replay_rpc_schema_compat_fixture(&fixture);
            let second = replay_rpc_schema_compat_fixture(&fixture);
            assert_eq!(first, second);
            assert_eq!(first.0, fixture.expected_processed_lines);
            assert_eq!(first.1, fixture.expected_error_count);
            assert_eq!(first.2, fixture.expected_responses);
        }
    }

    #[test]
    fn regression_rpc_schema_compat_dispatch_fixture_preserves_unsupported_schema_error_contract() {
        let fixture = load_rpc_schema_compat_fixture("dispatch-unsupported-continues.json");
        let (processed_lines, error_count, responses) = replay_rpc_schema_compat_fixture(&fixture);
        assert_eq!(processed_lines, 2);
        assert_eq!(error_count, 1);
        assert_eq!(responses, fixture.expected_responses);
        assert_eq!(responses[0]["kind"], "error");
        assert_eq!(
            responses[0]["payload"]["code"],
            RPC_ERROR_CODE_UNSUPPORTED_SCHEMA
        );
        assert_eq!(responses[1]["kind"], "run.accepted");
    }

    #[test]
    fn regression_rpc_schema_compat_serve_fixture_preserves_unsupported_schema_error_contract() {
        let fixture = load_rpc_schema_compat_fixture("serve-unsupported-continues.json");
        let (processed_lines, error_count, responses) = replay_rpc_schema_compat_fixture(&fixture);
        assert_eq!(processed_lines, 2);
        assert_eq!(error_count, 1);
        assert_eq!(responses, fixture.expected_responses);
        assert_eq!(responses[0]["kind"], "error");
        assert_eq!(
            responses[0]["payload"]["code"],
            RPC_ERROR_CODE_UNSUPPORTED_SCHEMA
        );
        assert_eq!(responses[1]["kind"], "run.accepted");
    }
}
