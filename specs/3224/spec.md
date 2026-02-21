# Spec: Issue #3224 - move gateway multi-channel status/report types into module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still defines multi-channel status/report structs and DTOs that are consumed only by `gateway_openresponses/multi_channel_status.rs`. This keeps the root module larger than needed and leaves internal status-modeling concerns in the root file.

## Scope
In scope:
- Extract multi-channel status/report structs + defaults into `multi_channel_status.rs`.
- Extract multi-channel runtime-state/event DTOs into `multi_channel_status.rs`.
- Ratchet and enforce root-module size guard.

Out of scope:
- Endpoint path/schema changes.
- Connector runtime behavior changes.
- Auth/rate-limit behavior changes.

## Acceptance Criteria
### AC-1 multi-channel status behavior remains contract-stable
Given existing unit/integration scenarios for multi-channel status collection,
when status is collected and `/gateway/status` is queried,
then health fields, counts, connector summaries, and diagnostics remain equivalent.

### AC-2 root module size and ownership boundaries improve
Given refactored module layout,
when running root-module size/ownership guard,
then root module is under tightened threshold and multi-channel status type definitions are no longer owned by root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Unit/Conformance | multi-channel fixture with runtime/connectors | `unit_collect_gateway_multi_channel_status_report_composes_runtime_and_connector_fields` | report health/count/connectors fields preserved |
| C-02 | AC-1 | Regression/Conformance | missing multi-channel state files | `regression_collect_gateway_multi_channel_status_report_defaults_when_state_is_missing` | fallback defaults/diagnostics preserved |
| C-03 | AC-1 | Integration/Conformance | gateway status endpoint fixture | `integration_gateway_status_endpoint_returns_expanded_multi_channel_health_payload` | `/gateway/status` multi-channel payload remains stable |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold and root ownership guards pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway unit_collect_gateway_multi_channel_status_report_composes_runtime_and_connector_fields`
- `cargo test -p tau-gateway regression_collect_gateway_multi_channel_status_report_defaults_when_state_is_missing`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_expanded_multi_channel_health_payload`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
