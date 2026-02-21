# Tasks: Issue #3152 - Correct Review #35 unresolved claims and add property rate-limit invariants

- [x] T1 (RED): add failing review conformance script checks for corrected unresolved rows (C-01, C-02).
- [x] T2 (RED): add failing property tests for rate-limit invariants in `tau-tools` (C-03, C-04, C-05).
- [x] T3 (GREEN): correct `tasks/review-35.md` unresolved tracker with current implementation evidence (C-01).
- [x] T4 (GREEN): implement property invariant checks using deterministic timestamp inputs (C-03, C-04, C-05).
- [x] T5 (VERIFY): run `scripts/dev/test-review-35.sh`, `cargo test -p tau-tools spec_3152 -- --test-threads=1`, `cargo fmt --check`, and scoped clippy.
