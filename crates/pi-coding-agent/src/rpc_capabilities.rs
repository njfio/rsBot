use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::rpc_protocol::RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY;
use crate::Cli;

pub(crate) const RPC_CAPABILITIES_SCHEMA_VERSION: u32 = 1;
pub(crate) const RPC_PROTOCOL_VERSION: &str = "0.1.0";

const RPC_CAPABILITIES: &[&str] = &[
    "errors.structured",
    "run.cancel",
    "run.complete",
    "run.fail",
    "run.start",
    "run.status",
    "run.timeout",
    "run.stream.assistant_text",
    "run.stream.tool_events",
];

pub(crate) fn rpc_capabilities_payload() -> Value {
    json!({
        "schema_version": RPC_CAPABILITIES_SCHEMA_VERSION,
        "protocol_version": RPC_PROTOCOL_VERSION,
        "capabilities": RPC_CAPABILITIES,
        "contracts": {
            "run_status": {
                "terminal_flag_always_present": true,
                "serve_closed_status_retention_capacity": RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY,
            }
        }
    })
}

pub(crate) fn execute_rpc_capabilities_command(cli: &Cli) -> Result<()> {
    if !cli.rpc_capabilities {
        return Ok(());
    }

    let payload = serde_json::to_string_pretty(&rpc_capabilities_payload())
        .context("failed to serialize rpc capabilities payload")?;
    println!("{payload}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::rpc_protocol::RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY;

    use super::{rpc_capabilities_payload, RPC_CAPABILITIES_SCHEMA_VERSION, RPC_PROTOCOL_VERSION};

    #[test]
    fn unit_rpc_capabilities_payload_has_expected_schema_and_version() {
        let payload = rpc_capabilities_payload();
        assert_eq!(
            payload["schema_version"].as_u64(),
            Some(RPC_CAPABILITIES_SCHEMA_VERSION as u64)
        );
        assert_eq!(
            payload["protocol_version"].as_str(),
            Some(RPC_PROTOCOL_VERSION)
        );
        assert_eq!(
            payload["contracts"]["run_status"]["terminal_flag_always_present"].as_bool(),
            Some(true)
        );
        assert_eq!(
            payload["contracts"]["run_status"]["serve_closed_status_retention_capacity"].as_u64(),
            Some(RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY as u64)
        );
    }

    #[test]
    fn functional_rpc_capabilities_payload_includes_deterministic_capabilities() {
        let payload = rpc_capabilities_payload();
        let capabilities = payload["capabilities"]
            .as_array()
            .expect("capabilities should be an array")
            .iter()
            .map(|value| value.as_str().expect("capability should be string"))
            .collect::<Vec<_>>();
        assert_eq!(
            capabilities,
            vec![
                "errors.structured",
                "run.cancel",
                "run.complete",
                "run.fail",
                "run.start",
                "run.status",
                "run.timeout",
                "run.stream.assistant_text",
                "run.stream.tool_events",
            ]
        );
    }

    #[test]
    fn regression_rpc_capabilities_payload_has_unique_entries() {
        let payload = rpc_capabilities_payload();
        let capabilities = payload["capabilities"]
            .as_array()
            .expect("capabilities should be an array")
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .expect("capability should be string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let unique = capabilities.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), capabilities.len());
    }
}
