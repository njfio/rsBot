# M246 - gateway openresponses execution handler modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still contains the large `execute_openresponses_request` function. Extracting it into a dedicated module isolates execution orchestration and significantly reduces root-module density.

## Scope
- Move `execute_openresponses_request` into a dedicated module.
- Preserve openresponses generation, usage accounting, and stream integration contracts.
- Ratchet root-module size/ownership guard.

## Linked Issues
- Epic: #3274
- Story: #3275
- Task: #3276

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_openresponses_endpoint_returns_non_stream_response`
- `cargo test -p tau-gateway functional_openresponses_endpoint_streams_sse_for_stream_true`
- `cargo test -p tau-gateway integration_spec_c01_openresponses_request_persists_session_usage_summary`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
