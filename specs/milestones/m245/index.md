# M245 - gateway openresponses entry handler modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still contains the primary HTTP entry handler `handle_openresponses`. Extracting this handler further isolates endpoint entry orchestration from module root.

## Scope
- Move `handle_openresponses` into a dedicated module.
- Preserve stream/non-stream behavior, auth, and request validation contracts.
- Ratchet root-module size/ownership guard.

## Linked Issues
- Epic: #3270
- Story: #3271
- Task: #3272

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_openresponses_endpoint_returns_non_stream_response`
- `cargo test -p tau-gateway functional_openresponses_endpoint_streams_sse_for_stream_true`
- `cargo test -p tau-gateway regression_openresponses_endpoint_rejects_oversized_input`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
