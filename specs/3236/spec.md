# Spec: Issue #3236 - move gateway endpoint/path constants into module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` contains a large set of endpoint/path constants that contribute significant size and cognitive load to the root module. These constants are stable and can be centralized in a dedicated module.

## Scope
In scope:
- Extract endpoint/path constants from `gateway_openresponses.rs` into `gateway_openresponses/endpoints.rs`.
- Preserve root/sibling references and endpoint string values.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Any endpoint string changes.
- Route additions/removals.
- Handler behavior changes.

## Acceptance Criteria
### AC-1 endpoint contract behavior remains stable
Given existing integration scenarios that validate status payload endpoint fields and authenticated request flows,
when gateway tests run,
then endpoint values and route behavior remain unchanged.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when running root-module guard,
then root line count is under tightened threshold and selected endpoint constants are no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Integration/Conformance | standard status endpoint fixture | `integration_gateway_status_endpoint_returns_service_snapshot` | status payload endpoint references remain stable |
| C-02 | AC-1 | Integration/Conformance | openai compat status fixture | `integration_gateway_status_endpoint_reports_openai_compat_runtime_counters` | openai/gateway endpoint references remain stable |
| C-03 | AC-1 | Integration/Conformance | localhost-dev auth fixture | `integration_localhost_dev_mode_allows_requests_without_bearer_token` | request flow using endpoint constants remains stable |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold and ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_localhost_dev_mode_allows_requests_without_bearer_token`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
