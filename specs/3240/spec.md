# Spec: Issue #3240 - move gateway server config/state types into module

Status: Reviewed

## Problem Statement
`gateway_openresponses.rs` still owns gateway server config/state type definitions and basic state helper methods. This keeps core runtime-state concerns in the root module and slows decomposition.

## Scope
In scope:
- Move `GatewayOpenResponsesServerConfig` and `GatewayOpenResponsesServerState` (plus state helper methods) into `gateway_openresponses/server_state.rs`.
- Re-export `GatewayOpenResponsesServerConfig` from root to preserve API path.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Config field/semantic changes.
- Route or endpoint behavior changes.

## Acceptance Criteria
### AC-1 runtime behavior remains stable
Given existing integration scenarios for status and authenticated request flows,
when tests run,
then behavior using shared server state/config remains unchanged.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when running size/ownership guard,
then root line count is under tightened threshold and server config/state type definitions are no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Integration/Conformance | status endpoint fixture | `integration_gateway_status_endpoint_returns_service_snapshot` | status behavior unchanged |
| C-02 | AC-1 | Integration/Conformance | compat status fixture | `integration_gateway_status_endpoint_reports_openai_compat_runtime_counters` | compat runtime counters/status unchanged |
| C-03 | AC-1 | Integration/Conformance | localhost-dev auth fixture | `integration_localhost_dev_mode_allows_requests_without_bearer_token` | authenticated request flow unchanged |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_localhost_dev_mode_allows_requests_without_bearer_token`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
