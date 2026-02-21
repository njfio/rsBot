# Tasks: Issue #3280 - move gateway root utility helpers to dedicated module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and assert helper utilities are not declared in root; run expecting failure.
- [x] T2 (GREEN): move helper utilities from `gateway_openresponses.rs` into `root_utilities.rs`; wire root imports.
- [x] T3 (VERIFY): run targeted helper conformance tests + guard.
- [x] T4 (VERIFY): run `cargo fmt --check` and `cargo clippy -- -D warnings`.
