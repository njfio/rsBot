# Tasks: Issue #3280 - move gateway root utility helpers to dedicated module

- [ ] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and assert helper utilities are not declared in root; run expecting failure.
- [ ] T2 (GREEN): move helper utilities from `gateway_openresponses.rs` into `root_utilities.rs`; wire root imports.
- [ ] T3 (VERIFY): run targeted helper conformance tests + guard.
- [ ] T4 (VERIFY): run `cargo fmt --check` and `cargo clippy -- -D warnings`.
