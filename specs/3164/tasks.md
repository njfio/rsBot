# Tasks: Issue #3164 - tau-training-proxy malformed-header and attribution-log resilience conformance

- [ ] T1 (RED): add C-01..C-04 conformance tests in `crates/tau-training-proxy/src/lib.rs` and run `cargo test -p tau-training-proxy spec_3164 -- --test-threads=1` expecting at least one failure.
- [ ] T2 (GREEN): implement minimal log append recovery fix (recreate missing parent directory) and rerun targeted conformance tests to pass.
- [ ] T3 (VERIFY): run `cargo test -p tau-training-proxy`, `cargo fmt --check`, `cargo clippy -p tau-training-proxy -- -D warnings`.
