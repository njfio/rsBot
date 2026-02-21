# M231 - gateway status handler modularization

Status: In Progress

## Context
After extracting events status logic, `gateway_openresponses.rs` still contains the full `/gateway/status` handler inline. This handler is large and can be moved into a focused module without changing endpoint behavior.

## Scope
- Extract `handle_gateway_status` into `gateway_openresponses/status_runtime.rs`.
- Preserve `GET /gateway/status` contract behavior (events/service/runtime fields).
- Tighten the gateway root module size guard threshold and keep it passing.

## Linked Issues
- Epic: #3215
- Story: #3214
- Task: #3216

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_events_status_when_configured`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
