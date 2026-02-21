# M238 - gateway bootstrap modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still directly owns gateway bootstrap/server wiring (`run_gateway_openresponses_server`) and router construction (`build_gateway_openresponses_router`). This keeps startup and routing orchestration tightly coupled to root module logic.

## Scope
- Move gateway bootstrap/server wiring and router builder into a dedicated module.
- Preserve root API path for `run_gateway_openresponses_server`.
- Ratchet root-module size/ownership guard.

## Linked Issues
- Epic: #3242
- Story: #3243
- Task: #3244

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_localhost_dev_mode_allows_requests_without_bearer_token`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
