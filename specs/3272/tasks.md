# Tasks: Issue #3272 - move openresponses entry handler to dedicated module

- [x] T1 (RED): tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and assert `handle_openresponses` is not declared in root; run expecting failure.
- [x] T2 (GREEN): move `handle_openresponses` from `gateway_openresponses.rs` into `openresponses_entry_handler.rs`; wire root imports.
- [x] T3 (VERIFY): run targeted openresponses entry conformance tests + guard.
- [x] T4 (VERIFY): run `cargo fmt --check` and `cargo clippy -- -D warnings`.
