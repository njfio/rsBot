# Tasks: Issue #2846 - /ops/sessions/{session_key} session graph node/edge contracts

1. [x] T1 (RED): add failing `functional_spec_2846_*` UI tests for graph panel, summary counts, node/edge rows, and empty state.
2. [x] T2 (RED): add failing `functional_spec_2846_*` and `integration_spec_2846_*` gateway tests for `/ops/sessions/{session_key}` graph contracts.
3. [x] T3 (GREEN): implement `tau-dashboard-ui` graph marker structs + deterministic SSR graph rendering.
4. [x] T4 (GREEN): implement gateway graph snapshot derivation from selected session lineage parent links.
5. [x] T5 (REGRESSION): rerun `spec_2842`, `spec_2838`, and `spec_2834` suites and fix regressions.
6. [x] T6 (VERIFY): run fmt/clippy/scoped tests/mutation and fast live validation.

## Verification Summary
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
- `cargo test -p tau-dashboard-ui functional_spec_2846 -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2846' -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2842 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2838 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2834 -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2842' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2838' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2834' -- --test-threads=1`
- `python3 .github/scripts/oversized_file_guard.py`
- `cargo test -p tau-dashboard-ui`
- `cargo test -p tau-gateway`
- `cargo mutants --in-diff /tmp/mutants_2846.diff -p tau-dashboard-ui -p tau-gateway`
- `./scripts/dev/fast-validate.sh --skip-fmt --check-only --direct-packages-only --full`
