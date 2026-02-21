# Tasks: Issue #3248 - move gateway ops shell handlers into module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` to `680` and assert ops shell macro/detail handlers are not declared in root; run expecting failure.
- [x] T2 (GREEN): move ops shell macro/handlers from `gateway_openresponses.rs` into `ops_shell_handlers.rs`; wire root imports.
- [x] T3 (VERIFY): run guard, targeted integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
