use std::io::BufReader;

use anyhow::{bail, Context, Result};
use serde_json::json;
use tau_cli::Cli;

pub(crate) use tau_runtime::{
    dispatch_rpc_ndjson_input, dispatch_rpc_raw_with_error_envelope, serve_rpc_ndjson_reader,
    validate_rpc_frame_file,
};

const RPC_ERROR_KIND: &str = "error";
const RPC_ERROR_CODE_IO_ERROR: &str = "io_error";

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

fn build_error_response_frame(
    request_id: &str,
    code: &str,
    message: &str,
) -> tau_runtime::RpcResponseFrame {
    tau_runtime::RpcResponseFrame {
        schema_version: tau_runtime::RPC_FRAME_SCHEMA_VERSION,
        request_id: request_id.to_string(),
        kind: RPC_ERROR_KIND.to_string(),
        payload: json!({
            "code": code,
            "message": message,
        }),
    }
}
