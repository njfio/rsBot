use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::rpc_protocol::{rpc_error_contracts, RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY};
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
const RPC_RUN_STATUS_VALUES: &[&str] = &[
    "active",
    "inactive",
    "cancelled",
    "completed",
    "failed",
    "timed_out",
];
const RPC_RUN_TERMINAL_STATES: &[&str] = &["cancelled", "completed", "failed", "timed_out"];

pub(crate) fn rpc_capabilities_payload() -> Value {
    let error_codes = rpc_error_contracts()
        .iter()
        .map(|contract| {
            json!({
                "code": contract.code,
                "category": contract.category,
                "description": contract.description,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "schema_version": RPC_CAPABILITIES_SCHEMA_VERSION,
        "protocol_version": RPC_PROTOCOL_VERSION,
        "capabilities": RPC_CAPABILITIES,
        "contracts": {
            "run_status": {
                "terminal_flag_always_present": true,
                "serve_closed_status_retention_capacity": RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY,
                "status_values": RPC_RUN_STATUS_VALUES,
                "terminal_states": RPC_RUN_TERMINAL_STATES,
                "terminal_state_field_present_for_terminal_status": true,
            },
            "errors": {
                "codes": error_codes,
            },
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
        assert_eq!(
            payload["contracts"]["run_status"]["terminal_state_field_present_for_terminal_status"]
                .as_bool(),
            Some(true)
        );
        assert_eq!(
            payload["contracts"]["errors"]["codes"]
                .as_array()
                .map(|codes| codes.len()),
            Some(7)
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

    #[test]
    fn functional_rpc_capabilities_payload_run_status_contract_is_deterministic() {
        let payload = rpc_capabilities_payload();
        let status_values = payload["contracts"]["run_status"]["status_values"]
            .as_array()
            .expect("status values should be an array")
            .iter()
            .map(|value| value.as_str().expect("status value should be string"))
            .collect::<Vec<_>>();
        assert_eq!(
            status_values,
            vec![
                "active",
                "inactive",
                "cancelled",
                "completed",
                "failed",
                "timed_out",
            ]
        );

        let terminal_states = payload["contracts"]["run_status"]["terminal_states"]
            .as_array()
            .expect("terminal states should be an array")
            .iter()
            .map(|value| value.as_str().expect("terminal state should be string"))
            .collect::<Vec<_>>();
        assert_eq!(
            terminal_states,
            vec!["cancelled", "completed", "failed", "timed_out"]
        );
    }

    #[test]
    fn functional_rpc_capabilities_payload_error_taxonomy_is_deterministic() {
        let payload = rpc_capabilities_payload();
        let codes = payload["contracts"]["errors"]["codes"]
            .as_array()
            .expect("error codes should be an array");
        let ordered_codes = codes
            .iter()
            .map(|entry| entry["code"].as_str().expect("code should be string"))
            .collect::<Vec<_>>();
        assert_eq!(
            ordered_codes,
            vec![
                "invalid_json",
                "unsupported_schema",
                "unsupported_kind",
                "invalid_request_id",
                "invalid_payload",
                "io_error",
                "internal_error",
            ]
        );
        assert_eq!(
            codes[0]["category"].as_str(),
            Some("validation"),
            "first code category should remain stable"
        );
        assert_eq!(
            codes[1]["category"].as_str(),
            Some("compatibility"),
            "unsupported schema should remain compatibility category"
        );
    }

    #[test]
    fn regression_rpc_capabilities_payload_error_taxonomy_has_unique_codes() {
        let payload = rpc_capabilities_payload();
        let codes = payload["contracts"]["errors"]["codes"]
            .as_array()
            .expect("error codes should be an array")
            .iter()
            .map(|entry| {
                entry["code"]
                    .as_str()
                    .expect("code should be string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let unique = codes.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), codes.len());
    }

    #[test]
    fn regression_rpc_capabilities_payload_run_status_contract_has_unique_entries() {
        let payload = rpc_capabilities_payload();
        for field in ["status_values", "terminal_states"] {
            let entries = payload["contracts"]["run_status"][field]
                .as_array()
                .expect("run status contract field should be an array")
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .expect("run status contract entry should be string")
                        .to_string()
                })
                .collect::<Vec<_>>();
            let unique = entries.iter().cloned().collect::<BTreeSet<_>>();
            assert_eq!(unique.len(), entries.len(), "duplicates found in {field}");
        }
    }
}
