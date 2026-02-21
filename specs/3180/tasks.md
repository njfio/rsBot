# Tasks: Issue #3180 - enforce prompt_telemetry_v1 schema-version requirement

- [ ] T1 (RED): add C-01/C-02 tests in `crates/tau-diagnostics/src/lib.rs` and run `cargo test -p tau-diagnostics spec_3180 -- --test-threads=1` expecting at least one failure.
- [ ] T2 (GREEN): implement minimal v1 compatibility predicate fix and rerun targeted conformance tests.
- [ ] T3 (VERIFY): run `cargo test -p tau-diagnostics`, `cargo fmt --check`, `cargo clippy -p tau-diagnostics -- -D warnings`.
