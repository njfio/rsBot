# Tasks: Issue #2850 - command-center recent-cycles table contracts

1. [x] T1 (RED): add failing `functional_spec_2850_*` UI tests for panel, summary-row attributes, and empty/non-empty state markers.
2. [x] T2 (RED): add failing `functional_spec_2850_*` and `integration_spec_2850_*` gateway tests for `/ops` recent-cycles table contracts.
3. [x] T3 (GREEN): implement deterministic recent-cycles panel/summary-row/empty-row marker rendering in `tau-dashboard-ui`.
4. [x] T4 (REGRESSION): rerun `spec_2806`, `spec_2814`, `spec_2826`, and `spec_2818` suites and fix regressions.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation and fast live validation.

## Verification Summary
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
- `cargo test -p tau-dashboard-ui functional_spec_2850 -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2850' -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2806 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2814 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2826 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui functional_spec_2818 -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2806' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2814' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2826' -- --test-threads=1`
- `cargo test -p tau-gateway 'spec_2818' -- --test-threads=1`
- `python3 .github/scripts/oversized_file_guard.py`
- `cargo test -p tau-dashboard-ui`
- `cargo test -p tau-gateway`
- `cargo mutants --in-diff /tmp/mutants_2850.diff -p tau-dashboard-ui -p tau-gateway`
- `./scripts/dev/fast-validate.sh --skip-fmt --check-only --direct-packages-only --full`
