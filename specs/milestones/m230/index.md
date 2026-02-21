# M230 - gateway events status modularization

Status: In Progress

## Context
`crates/tau-gateway/src/gateway_openresponses.rs` remains a concentration point despite prior decomposition. Event scheduler status report logic currently lives inline in that file and can be isolated into a dedicated module without changing the `GET /gateway/status` contract.

## Scope
- Extract events status types and collection logic into `gateway_openresponses/events_status.rs`.
- Keep `/gateway/status` `events` payload behavior stable.
- Add a deterministic guard to prevent regression in hotspot size pressure.

## Linked Issues
- Epic: #3211
- Story: #3210
- Task: #3212

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_events_status_when_configured`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
