# Tasks: Issue #3236 - move gateway endpoint/path constants into module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` to `1110` and assert selected endpoint constants are not declared in root; run expecting failure.
- [x] T2 (GREEN): move endpoint/path constants from `gateway_openresponses.rs` into `endpoints.rs` and rewire root references.
- [x] T3 (VERIFY): run guard, targeted integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
