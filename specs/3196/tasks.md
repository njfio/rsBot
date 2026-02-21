# Tasks: Issue #3196 - make panic/unsafe audit test-context aware

- [x] T1 (RED): add src-level `#[cfg(test)]` panic/unsafe fixture markers and update expected fixture assertions; run `scripts/dev/test-audit-panic-unsafe.sh` expecting failure.
- [x] T2 (GREEN): implement test-context-aware classification in `scripts/dev/audit-panic-unsafe.sh` and rerun fixture test to pass.
- [x] T3 (VERIFY): run `scripts/dev/test-audit-panic-unsafe.sh`, `cargo fmt --check`, `cargo clippy -- -D warnings`.
