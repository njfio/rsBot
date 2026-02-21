# Spec: Issue #3244 - move gateway bootstrap/router wiring into module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still contains bootstrap/runtime startup flow and large router assembly wiring. Keeping these orchestration responsibilities in root increases file size and coupling, slowing future decomposition.

## Scope
In scope:
- Move `run_gateway_openresponses_server` and `build_gateway_openresponses_router` from root into `gateway_openresponses/server_bootstrap.rs`.
- Preserve public API path for `run_gateway_openresponses_server` from root module.
- Ratchet and enforce root size/ownership guard for moved functions.

Out of scope:
- Route additions/removals.
- Endpoint string/value changes.
- Runtime behavior changes to auth/session/status flows.

## Acceptance Criteria
### AC-1 runtime behavior remains stable
Given existing integration scenarios for status reporting and localhost-dev auth,
when gateway tests run,
then request/response behavior and status payload contracts remain unchanged.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when running root-module guard,
then root line count is under tightened threshold and bootstrap/router function definitions are no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Integration/Conformance | status endpoint fixture | `integration_gateway_status_endpoint_returns_service_snapshot` | service snapshot/status payload behavior remains stable |
| C-02 | AC-1 | Integration/Conformance | openai compat status fixture | `integration_gateway_status_endpoint_reports_openai_compat_runtime_counters` | compat runtime counters/status behavior remains stable |
| C-03 | AC-1 | Integration/Conformance | localhost-dev auth fixture | `integration_localhost_dev_mode_allows_requests_without_bearer_token` | localhost dev-mode request flow remains stable |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_localhost_dev_mode_allows_requests_without_bearer_token`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
