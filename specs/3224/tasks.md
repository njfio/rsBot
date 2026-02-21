# Tasks: Issue #3224 - move gateway multi-channel status/report types into module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` to `1300` and add guard that multi-channel status type definitions are not in root; run expecting failure.
- [x] T2 (GREEN): move multi-channel status/report structs + defaults and state/event DTOs from `gateway_openresponses.rs` to `multi_channel_status.rs`; rewire visibility/imports.
- [x] T3 (VERIFY): run size guard, targeted multi-channel unit/regression/integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
