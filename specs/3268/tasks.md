# Tasks: Issue #3268 - move auth-session handler to dedicated module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and assert `handle_gateway_auth_session` is not declared in root; run expecting failure.
- [x] T2 (GREEN): move `handle_gateway_auth_session` from `gateway_openresponses.rs` into `auth_session_handler.rs`; wire root imports.
- [x] T3 (VERIFY): run targeted auth-session conformance tests + guard.
- [x] T4 (VERIFY): run `cargo fmt --check` and `cargo clippy -- -D warnings`.
