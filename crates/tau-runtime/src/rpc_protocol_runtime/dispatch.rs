use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::{
    build_response_frame, forget_closed_run_state, remember_closed_run_state,
    require_non_empty_payload_string, resolve_capabilities_requested_schema_version,
    resolve_run_fail_reason, resolve_run_start_run_id, resolve_run_timeout_reason,
    rpc_capabilities_payload, RpcFrame, RpcFrameKind, RpcResponseFrame, RpcServeSessionState,
    RpcTerminalRunState, LIFECYCLE_CONTROL_ACTION_CANCEL, LIFECYCLE_CONTROL_ACTION_RESUME,
    LIFECYCLE_CONTROL_AUDIT_RECORD_TYPE_V1, LIFECYCLE_CONTROL_AUDIT_SCHEMA_VERSION,
    LIFECYCLE_CONTROL_STATUS_ACCEPTED, RPC_COMPATIBLE_REQUEST_SCHEMA_VERSIONS,
    RPC_FRAME_SCHEMA_VERSION, RPC_PROTOCOL_VERSION, RPC_STUB_MODE,
};

pub fn dispatch_rpc_frame_impl(frame: &RpcFrame) -> Result<RpcResponseFrame> {
    match frame.kind {
        RpcFrameKind::CapabilitiesRequest => {
            let negotiated_request_schema_version =
                resolve_capabilities_requested_schema_version(&frame.payload)?;
            let capabilities = rpc_capabilities_payload();
            let capability_list = capabilities["capabilities"]
                .as_array()
                .cloned()
                .ok_or_else(|| anyhow!("rpc capabilities payload is missing capabilities array"))?;
            let contracts = capabilities["contracts"]
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("rpc capabilities payload is missing contracts object"))?;
            Ok(build_response_frame(
                &frame.request_id,
                "capabilities.response",
                json!({
                    "protocol_version": RPC_PROTOCOL_VERSION,
                    "response_schema_version": RPC_FRAME_SCHEMA_VERSION,
                    "supported_request_schema_versions": RPC_COMPATIBLE_REQUEST_SCHEMA_VERSIONS,
                    "negotiated_request_schema_version": negotiated_request_schema_version,
                    "capabilities": capability_list,
                    "contracts": contracts,
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

fn current_unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn build_lifecycle_control_audit_record(
    request_id: &str,
    run_id: &str,
    action: &str,
    from_state: &str,
    to_state: &str,
) -> Value {
    json!({
        "record_type": LIFECYCLE_CONTROL_AUDIT_RECORD_TYPE_V1,
        "schema_version": LIFECYCLE_CONTROL_AUDIT_SCHEMA_VERSION,
        "timestamp_unix_ms": current_unix_timestamp_ms(),
        "request_id": request_id,
        "run_id": run_id,
        "action": action,
        "from_state": from_state,
        "to_state": to_state,
        "status": LIFECYCLE_CONTROL_STATUS_ACCEPTED,
    })
}

pub(super) fn lifecycle_control_audit_record_for_dispatch_impl(
    frame: &RpcFrame,
    responses: &[RpcResponseFrame],
) -> Option<Value> {
    match frame.kind {
        RpcFrameKind::RunStart => {
            let run_id = responses
                .first()
                .and_then(|response| response.payload.get("run_id"))
                .and_then(Value::as_str)?;
            Some(build_lifecycle_control_audit_record(
                &frame.request_id,
                run_id,
                LIFECYCLE_CONTROL_ACTION_RESUME,
                "inactive",
                "running",
            ))
        }
        RpcFrameKind::RunCancel => {
            let run_id = frame.payload.get("run_id").and_then(Value::as_str)?;
            Some(build_lifecycle_control_audit_record(
                &frame.request_id,
                run_id,
                LIFECYCLE_CONTROL_ACTION_CANCEL,
                "running",
                "cancelled",
            ))
        }
        _ => None,
    }
}

pub(super) fn dispatch_rpc_frame_for_serve_impl(
    frame: &RpcFrame,
    state: &mut RpcServeSessionState,
) -> Result<Vec<RpcResponseFrame>> {
    match frame.kind {
        RpcFrameKind::CapabilitiesRequest => Ok(vec![dispatch_rpc_frame_impl(frame)?]),
        RpcFrameKind::RunStart => {
            let response = dispatch_rpc_frame_impl(frame)?;
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
            responses.extend(super::build_run_start_stream_frames(
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
            let mut responses = vec![dispatch_rpc_frame_impl(frame)?];
            responses.push(super::build_run_cancel_stream_frame(
                &frame.request_id,
                &run_id,
            ));
            responses.push(super::build_run_cancel_assistant_stream_frame(
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
            let mut responses = vec![dispatch_rpc_frame_impl(frame)?];
            responses.push(super::build_run_complete_stream_frame(
                &frame.request_id,
                &run_id,
            ));
            responses.push(super::build_run_complete_assistant_stream_frame(
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
            let mut responses = vec![dispatch_rpc_frame_impl(frame)?];
            responses.push(super::build_run_failed_stream_frame(
                &frame.request_id,
                &run_id,
                &reason,
            ));
            responses.push(super::build_run_failed_assistant_stream_frame(
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
            let mut responses = vec![dispatch_rpc_frame_impl(frame)?];
            responses.push(super::build_run_timeout_stream_frame(
                &frame.request_id,
                &run_id,
                &reason,
            ));
            responses.push(super::build_run_timeout_assistant_stream_frame(
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
