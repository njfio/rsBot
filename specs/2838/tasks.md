# Tasks: Issue #2838 - Sessions explorer deterministic row contracts

## Ordered Tasks
1. [x] T1 (RED): add conformance tests for `/ops/sessions` panel/list/row/empty-state SSR markers in UI and gateway.
2. [x] T2 (GREEN): implement sessions explorer snapshot/render mapping in UI + gateway.
3. [x] T3 (REGRESSION): run phase 1N chat selector suites and ops route marker regressions.
4. [x] T4 (VERIFY): run fmt/clippy/tests/guardrails/mutation and set spec status `Implemented`.

## Tier Mapping
- Unit: selector/sessions helper behavior where isolated.
- Property: N/A.
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: `/ops/sessions` marker contracts.
- Conformance: C-01..C-04.
- Integration: discovered session rows + preserved row href controls.
- Fuzz: N/A.
- Mutation: `cargo mutants --in-diff <diff-file> -p tau-dashboard-ui -p tau-gateway`.
- Regression: phase 1N and route-shell suites.
- Performance: N/A.

## Verification Evidence
- Targeted:
  - `cargo test -p tau-dashboard-ui functional_spec_2838 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2838 -- --test-threads=1`
  - `cargo test -p tau-gateway integration_spec_2838 -- --test-threads=1`
- Regression:
  - `cargo test -p tau-dashboard-ui functional_spec_2834 -- --test-threads=1`
  - `cargo test -p tau-gateway spec_2834 -- --test-threads=1`
- Verify:
  - `cargo fmt --check`
  - `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
  - `python3 .github/scripts/oversized_file_guard.py`
  - `cargo mutants --in-diff /tmp/mutants_2838.diff -p tau-dashboard-ui -p tau-gateway` (`3/4` caught, `1` unviable compile-time mutant)
  - `cargo test -p tau-dashboard-ui`
  - `cargo test -p tau-gateway`
