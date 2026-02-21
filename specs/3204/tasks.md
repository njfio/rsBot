# Tasks: Issue #3204 - align panic policy audit classifier with per-line test context

- [x] T1 (RED): extend `scripts/dev/test-panic-unsafe-audit.sh` fixture with post-cfg non-test panic and update expectations; run expecting failure.
- [x] T2 (GREEN): implement per-line test-context classification in `scripts/dev/panic-unsafe-audit.sh` and rerun audit fixture + guard fixture tests.
- [x] T3 (VERIFY): run `scripts/dev/test-panic-unsafe-audit.sh`, `scripts/dev/test-panic-unsafe-guard.sh`, `cargo fmt --check`, `cargo clippy -- -D warnings`.
