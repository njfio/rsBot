# M242 - gateway websocket-stream handlers modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still contains WebSocket upgrade and dashboard stream loop handlers. Extracting these handlers into a dedicated module keeps root focused on routing and request orchestration.

## Scope
- Move WebSocket upgrade and dashboard stream-loop helpers into a dedicated module.
- Preserve ws/dashboard stream behavior and contracts.
- Ratchet root-module size/ownership guard.

## Linked Issues
- Epic: #3258
- Story: #3259
- Task: #3260

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_gateway_ws_upgrade_missing_session_token_returns_unauthorized`
- `cargo test -p tau-gateway functional_gateway_ws_upgrade_includes_session_token_when_present`
- `cargo test -p tau-gateway functional_dashboard_stream_returns_sse_when_requested`
- `cargo test -p tau-gateway functional_dashboard_stream_preserves_id_counter_across_events`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
