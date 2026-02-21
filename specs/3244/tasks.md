# Tasks: Issue #3244 - move gateway bootstrap/router wiring into module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` to `860` and assert bootstrap/router function definitions are not declared in root; run expecting failure.
- [x] T2 (GREEN): move bootstrap and router wiring functions from `gateway_openresponses.rs` into `server_bootstrap.rs`; re-export startup fn from root.
- [x] T3 (VERIFY): run guard, targeted integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
