# Tasks: Issue #3260 - move websocket and stream handlers to dedicated module

- [ ] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and assert moved ws/stream handlers are not declared in root; run expecting failure.
- [ ] T2 (GREEN): move ws/stream handlers from `gateway_openresponses.rs` into `ws_stream_handlers.rs`; wire root imports.
- [ ] T3 (VERIFY): run targeted ws/stream conformance tests + guard.
- [ ] T4 (VERIFY): run `cargo fmt --check` and `cargo clippy -- -D warnings`.
