# M239 - gateway ops shell handlers modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still owns Ops dashboard shell handler macro and shell route glue functions. This UI route wrapper logic can be isolated to reduce root-module coupling and keep decomposition momentum.

## Scope
- Move Ops shell macro/handlers to a dedicated module.
- Preserve route handler behavior and endpoint wiring.
- Ratchet root-module size/ownership guard.

## Linked Issues
- Epic: #3246
- Story: #3247
- Task: #3248

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_localhost_dev_mode_allows_requests_without_bearer_token`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
