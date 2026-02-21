# Tasks: Issue #3220 - move gateway compat/telemetry runtime state into module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` to `1450` and require compat-state module wiring; run expecting failure.
- [x] T2 (GREEN): extract compat/telemetry runtime state/types/methods from `gateway_openresponses.rs` into `compat_state_runtime.rs` and rewire.
- [x] T3 (VERIFY): run size guard, targeted compat/telemetry integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
