# Tasks: Issue #3216 - move /gateway/status handler into status_runtime module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and status-module wiring assertions; run expecting failure.
- [x] T2 (GREEN): extract `handle_gateway_status` into `gateway_openresponses/status_runtime.rs` and rewire module imports/routes.
- [x] T3 (VERIFY): run size guard, two gateway status integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
