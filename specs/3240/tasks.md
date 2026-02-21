# Tasks: Issue #3240 - move gateway server config/state types into module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` to `1040` and assert server config/state types are not declared in root; run expecting failure.
- [x] T2 (GREEN): move server config/state structs and helper methods from `gateway_openresponses.rs` into `server_state.rs`; re-export config type from root.
- [x] T3 (VERIFY): run guard, targeted integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
