# Tasks: Issue #2854 - command-center route visibility contracts

1. [x] T1 (RED): add failing `functional_spec_2854_*` UI tests for `/ops` visible and non-command-center hidden marker contracts.
2. [x] T2 (RED): add failing `functional_spec_2854_*` and `integration_spec_2854_*` gateway tests for route visibility contracts.
3. [x] T3 (GREEN): implement route-aware command-center visibility markers in `tau-dashboard-ui`.
4. [x] T4 (REGRESSION): rerun `spec_2806`, `spec_2830`, `spec_2838`, and `spec_2842` suites and fix regressions.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation and fast live validation.

## Verification Summary
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
- `cargo test -p tau-dashboard-ui functional_spec_2854 -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2854' -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2806 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2830 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2838 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2842 -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2806' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2830' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2838' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2842' -- --test-threads=1`
- `python3 .github/scripts/oversized_file_guard.py`
- `cargo test -p tau-dashboard-ui`
- `cargo test -p tau-gateway`
- `cargo mutants --in-diff /tmp/mutants_2854.diff -p tau-dashboard-ui -p tau-gateway`
- `./scripts/dev/fast-validate.sh --skip-fmt --check-only --direct-packages-only --full`
