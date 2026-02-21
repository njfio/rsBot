# Spec: Issue #3248 - move gateway ops shell handlers into module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still contains Ops dashboard shell macro/handler glue. Keeping this shell route wrapper logic in root increases file size and mixes dashboard shell concerns with core runtime handlers.

## Scope
In scope:
- Move Ops shell macro and related handler functions from root into `gateway_openresponses/ops_shell_handlers.rs`.
- Keep existing route behavior and endpoint mapping unchanged.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Dashboard endpoint changes.
- Shell rendering behavior changes.
- Auth/session/runtime logic changes.

## Acceptance Criteria
### AC-1 runtime behavior remains stable
Given existing status/auth integration scenarios,
when gateway tests run,
then gateway endpoint behavior remains unchanged after handler extraction.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when running root guard,
then root line count is under tightened threshold and ops shell macro/detail handler definitions are no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Integration/Conformance | status endpoint fixture | `integration_gateway_status_endpoint_returns_service_snapshot` | status behavior remains stable |
| C-02 | AC-1 | Integration/Conformance | compat status fixture | `integration_gateway_status_endpoint_reports_openai_compat_runtime_counters` | compat counters/status remain stable |
| C-03 | AC-1 | Integration/Conformance | localhost-dev auth fixture | `integration_localhost_dev_mode_allows_requests_without_bearer_token` | auth flow remains stable |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ops shell ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_localhost_dev_mode_allows_requests_without_bearer_token`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
