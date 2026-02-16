use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::rpc_protocol_runtime::{rpc_error_contracts, RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY};

pub const RPC_CAPABILITIES_SCHEMA_VERSION: u32 = 1;
pub const RPC_PROTOCOL_VERSION: &str = "0.1.0";

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
const RPC_PROTOCOL_REQUEST_KINDS: &[&str] = &[
    "capabilities.request",
    "run.start",
    "run.cancel",
    "run.complete",
    "run.fail",
    "run.timeout",
    "run.status",
];
const RPC_PROTOCOL_RESPONSE_KINDS: &[&str] = &[
    "capabilities.response",
    "run.accepted",
    "run.cancelled",
    "run.completed",
    "run.failed",
    "run.timed_out",
    "run.status",
    "run.stream.tool_events",
    "run.stream.assistant_text",
    "error",
];
const RPC_PROTOCOL_STREAM_EVENT_KINDS: &[&str] =
    &["run.stream.tool_events", "run.stream.assistant_text"];
const RPC_LIFECYCLE_NON_TERMINAL_TRANSITIONS: &[(&str, &str, &[&str])] = &[
    (
        "run.start",
        "run.accepted",
        &["run.stream.tool_events", "run.stream.assistant_text"],
    ),
    ("run.status", "run.status", &[]),
];
const RPC_LIFECYCLE_TERMINAL_TRANSITIONS: &[(&str, &str, &str, &str)] = &[
    ("run.cancel", "run.cancelled", "cancelled", "run.cancelled"),
    (
        "run.complete",
        "run.completed",
        "completed",
        "run.completed",
    ),
    ("run.fail", "run.failed", "failed", "run.failed"),
    ("run.timeout", "run.timed_out", "timed_out", "run.timed_out"),
];

/// Build canonical RPC capabilities payload for CLI and NDJSON protocol clients.
pub fn rpc_capabilities_payload() -> Value {
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
    let lifecycle_non_terminal_transitions = RPC_LIFECYCLE_NON_TERMINAL_TRANSITIONS
        .iter()
        .map(|(request_kind, response_kind, stream_event_kinds)| {
            json!({
                "request_kind": request_kind,
                "response_kind": response_kind,
                "stream_event_kinds": stream_event_kinds,
            })
        })
        .collect::<Vec<_>>();
    let lifecycle_transitions = RPC_LIFECYCLE_TERMINAL_TRANSITIONS
        .iter()
        .map(
            |(request_kind, response_kind, terminal_state, terminal_stream_tool_event)| {
                json!({
                    "request_kind": request_kind,
                    "response_kind": response_kind,
                    "terminal_state": terminal_state,
                    "terminal_stream_tool_event": terminal_stream_tool_event,
                })
            },
        )
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
            "protocol": {
                "request_kinds": RPC_PROTOCOL_REQUEST_KINDS,
                "response_kinds": RPC_PROTOCOL_RESPONSE_KINDS,
                "stream_event_kinds": RPC_PROTOCOL_STREAM_EVENT_KINDS,
            },
            "lifecycle": {
                "terminal_assistant_stream_final_required": true,
                "non_terminal_transitions": lifecycle_non_terminal_transitions,
                "terminal_transitions": lifecycle_transitions,
            },
        }
    })
}

/// Render RPC capabilities payload as pretty JSON text.
pub fn render_rpc_capabilities_payload_pretty() -> Result<String> {
    serde_json::to_string_pretty(&rpc_capabilities_payload())
        .context("failed to serialize rpc capabilities payload")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use crate::rpc_protocol_runtime::RPC_SERVE_CLOSED_RUN_STATUS_CAPACITY;

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
        assert_eq!(
            payload["contracts"]["protocol"]["request_kinds"]
                .as_array()
                .map(|kinds| kinds.len()),
            Some(7)
        );
        assert_eq!(
            payload["contracts"]["protocol"]["response_kinds"]
                .as_array()
                .map(|kinds| kinds.len()),
            Some(10)
        );
        assert_eq!(
            payload["contracts"]["lifecycle"]["terminal_transitions"]
                .as_array()
                .map(|transitions| transitions.len()),
            Some(4)
        );
        assert_eq!(
            payload["contracts"]["lifecycle"]["non_terminal_transitions"]
                .as_array()
                .map(|transitions| transitions.len()),
            Some(2)
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

    #[test]
    fn functional_rpc_capabilities_payload_protocol_kinds_are_deterministic() {
        let payload = rpc_capabilities_payload();
        let request_kinds = payload["contracts"]["protocol"]["request_kinds"]
            .as_array()
            .expect("request kinds should be an array")
            .iter()
            .map(|kind| kind.as_str().expect("request kind should be string"))
            .collect::<Vec<_>>();
        assert_eq!(
            request_kinds,
            vec![
                "capabilities.request",
                "run.start",
                "run.cancel",
                "run.complete",
                "run.fail",
                "run.timeout",
                "run.status",
            ]
        );

        let stream_event_kinds = payload["contracts"]["protocol"]["stream_event_kinds"]
            .as_array()
            .expect("stream event kinds should be an array")
            .iter()
            .map(|kind| kind.as_str().expect("stream event kind should be string"))
            .collect::<Vec<_>>();
        assert_eq!(
            stream_event_kinds,
            vec!["run.stream.tool_events", "run.stream.assistant_text"]
        );
    }

    #[test]
    fn functional_rpc_capabilities_payload_lifecycle_transitions_are_deterministic() {
        let payload = rpc_capabilities_payload();
        assert_eq!(
            payload["contracts"]["lifecycle"]["terminal_assistant_stream_final_required"].as_bool(),
            Some(true)
        );
        let non_terminal_transitions = payload["contracts"]["lifecycle"]
            ["non_terminal_transitions"]
            .as_array()
            .expect("lifecycle non-terminal transitions should be an array");
        assert_eq!(non_terminal_transitions.len(), 2);
        assert_eq!(
            non_terminal_transitions[0],
            json!({
                "request_kind": "run.start",
                "response_kind": "run.accepted",
                "stream_event_kinds": ["run.stream.tool_events", "run.stream.assistant_text"],
            })
        );
        assert_eq!(
            non_terminal_transitions[1],
            json!({
                "request_kind": "run.status",
                "response_kind": "run.status",
                "stream_event_kinds": [],
            })
        );
        let transitions = payload["contracts"]["lifecycle"]["terminal_transitions"]
            .as_array()
            .expect("lifecycle transitions should be an array");
        assert_eq!(transitions.len(), 4);
        assert_eq!(
            transitions[0],
            json!({
                "request_kind": "run.cancel",
                "response_kind": "run.cancelled",
                "terminal_state": "cancelled",
                "terminal_stream_tool_event": "run.cancelled",
            })
        );
        assert_eq!(
            transitions[1],
            json!({
                "request_kind": "run.complete",
                "response_kind": "run.completed",
                "terminal_state": "completed",
                "terminal_stream_tool_event": "run.completed",
            })
        );
        assert_eq!(
            transitions[2],
            json!({
                "request_kind": "run.fail",
                "response_kind": "run.failed",
                "terminal_state": "failed",
                "terminal_stream_tool_event": "run.failed",
            })
        );
        assert_eq!(
            transitions[3],
            json!({
                "request_kind": "run.timeout",
                "response_kind": "run.timed_out",
                "terminal_state": "timed_out",
                "terminal_stream_tool_event": "run.timed_out",
            })
        );
    }

    #[test]
    fn regression_rpc_capabilities_payload_protocol_kind_lists_have_unique_entries() {
        let payload = rpc_capabilities_payload();
        for field in ["request_kinds", "response_kinds", "stream_event_kinds"] {
            let entries = payload["contracts"]["protocol"][field]
                .as_array()
                .expect("protocol kind list should be an array")
                .iter()
                .map(|kind| {
                    kind.as_str()
                        .expect("protocol kind should be string")
                        .to_string()
                })
                .collect::<Vec<_>>();
            let unique = entries.iter().cloned().collect::<BTreeSet<_>>();
            assert_eq!(
                unique.len(),
                entries.len(),
                "duplicates found in protocol contract field {field}"
            );
        }
    }

    #[test]
    fn regression_rpc_capabilities_payload_lifecycle_transition_kinds_are_unique() {
        let payload = rpc_capabilities_payload();
        let non_terminal_transitions = payload["contracts"]["lifecycle"]
            ["non_terminal_transitions"]
            .as_array()
            .expect("lifecycle non-terminal transitions should be an array");
        let non_terminal_request_kinds = non_terminal_transitions
            .iter()
            .map(|entry| {
                entry["request_kind"]
                    .as_str()
                    .expect("request kind should be string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let unique_non_terminal_request_kinds = non_terminal_request_kinds
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            unique_non_terminal_request_kinds.len(),
            non_terminal_request_kinds.len()
        );
        let non_terminal_response_kinds = non_terminal_transitions
            .iter()
            .map(|entry| {
                entry["response_kind"]
                    .as_str()
                    .expect("response kind should be string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let unique_non_terminal_response_kinds = non_terminal_response_kinds
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            unique_non_terminal_response_kinds.len(),
            non_terminal_response_kinds.len()
        );
        for transition in non_terminal_transitions {
            let stream_event_kinds = transition["stream_event_kinds"]
                .as_array()
                .expect("stream event kinds should be an array")
                .iter()
                .map(|entry| {
                    entry
                        .as_str()
                        .expect("stream event kind should be string")
                        .to_string()
                })
                .collect::<Vec<_>>();
            let unique_stream_event_kinds =
                stream_event_kinds.iter().cloned().collect::<BTreeSet<_>>();
            assert_eq!(unique_stream_event_kinds.len(), stream_event_kinds.len());
        }

        let transitions = payload["contracts"]["lifecycle"]["terminal_transitions"]
            .as_array()
            .expect("lifecycle transitions should be an array");
        let request_kinds = transitions
            .iter()
            .map(|entry| {
                entry["request_kind"]
                    .as_str()
                    .expect("request kind should be string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let unique_request_kinds = request_kinds.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(unique_request_kinds.len(), request_kinds.len());

        let response_kinds = transitions
            .iter()
            .map(|entry| {
                entry["response_kind"]
                    .as_str()
                    .expect("response kind should be string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let unique_response_kinds = response_kinds.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(unique_response_kinds.len(), response_kinds.len());
    }
}
