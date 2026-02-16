use std::io::{BufRead, Write};

use anyhow::{Context, Result};

use super::{
    best_effort_request_id_from_raw, build_error_response_frame, classify_rpc_error_message,
    dispatch_rpc_frame, dispatch_rpc_frame_for_serve, lifecycle_control_audit_record_for_dispatch,
    parse_rpc_frame, RpcNdjsonDispatchReport, RpcNdjsonServeReport, RpcResponseFrame,
    RpcServeSessionState, RPC_ERROR_KIND,
};

/// Dispatch one raw RPC frame string and always return a response envelope.
pub fn dispatch_rpc_raw_with_error_envelope_impl(raw: &str) -> RpcResponseFrame {
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

pub fn dispatch_rpc_ndjson_input_impl(raw: &str) -> RpcNdjsonDispatchReport {
    let mut responses = Vec::new();
    let mut processed_lines = 0_usize;
    let mut error_count = 0_usize;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        processed_lines = processed_lines.saturating_add(1);
        let response = dispatch_rpc_raw_with_error_envelope_impl(trimmed);
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

/// Serve NDJSON RPC frames from a reader and stream responses to writer.
pub fn serve_rpc_ndjson_reader_impl<R, W>(
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

pub fn serve_rpc_ndjson_reader_with_lifecycle_audit_impl<R, W, A>(
    mut reader: R,
    writer: &mut W,
    lifecycle_audit_writer: &mut A,
) -> Result<RpcNdjsonServeReport>
where
    R: BufRead,
    W: Write,
    A: Write,
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

        let responses = match parse_rpc_frame(trimmed) {
            Ok(frame) => match dispatch_rpc_frame_for_serve(&frame, &mut state) {
                Ok(responses) => {
                    if let Some(audit_record) =
                        lifecycle_control_audit_record_for_dispatch(&frame, &responses)
                    {
                        serde_json::to_writer(&mut *lifecycle_audit_writer, &audit_record)
                            .context("failed to serialize lifecycle audit record")?;
                        lifecycle_audit_writer
                            .write_all(b"\n")
                            .context("failed to write lifecycle audit delimiter")?;
                        lifecycle_audit_writer
                            .flush()
                            .context("failed to flush lifecycle audit line")?;
                    }
                    responses
                }
                Err(error) => vec![build_error_response_frame(
                    &frame.request_id,
                    classify_rpc_error_message(&error.to_string()),
                    &error.to_string(),
                )],
            },
            Err(error) => {
                let request_id = best_effort_request_id_from_raw(trimmed)
                    .unwrap_or_else(|| "unknown".to_string());
                vec![build_error_response_frame(
                    &request_id,
                    classify_rpc_error_message(&error.to_string()),
                    &error.to_string(),
                )]
            }
        };

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
