# Tasks: Issue #3172 - training-proxy JSONL newline delimiter integrity

- [ ] T1 (RED): add C-01 conformance test in `crates/tau-training-proxy/src/lib.rs` and run `cargo test -p tau-training-proxy spec_3172 -- --test-threads=1` expecting failure.
- [ ] T2 (GREEN): implement minimal append delimiter fix and rerun targeted conformance tests to pass.
- [ ] T3 (VERIFY): run `cargo test -p tau-training-proxy`, `cargo fmt --check`, `cargo clippy -p tau-training-proxy -- -D warnings`.
