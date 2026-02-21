# Tasks: Issue #3264 - move stream_openresponses handler to dedicated module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and assert `stream_openresponses` is not declared in root; run expecting failure.
- [x] T2 (GREEN): move `stream_openresponses` from `gateway_openresponses.rs` into `stream_response_handler.rs`; wire root imports.
- [x] T3 (VERIFY): run targeted openresponses conformance tests + guard.
- [x] T4 (VERIFY): run `cargo fmt --check` and `cargo clippy -- -D warnings`.
