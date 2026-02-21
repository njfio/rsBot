# Tasks: Issue #3212 - extract gateway events status collector with contract parity

- [x] T1 (RED): add `scripts/dev/test-gateway-openresponses-size.sh` and run it expecting failure on current root module size/module layout.
- [x] T2 (GREEN): extract events status structs/collector to `gateway_openresponses/events_status.rs`, rewire root module imports, and satisfy the size guard.
- [x] T3 (VERIFY): run `scripts/dev/test-gateway-openresponses-size.sh`, targeted gateway status integration tests, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
