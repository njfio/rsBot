# Tasks: Issue #2834 - Chat active session selector contracts

## Ordered Tasks
1. [x] T1 (RED): add conformance tests for chat session selector SSR markers in `tau-dashboard-ui` and `/ops/chat` gateway rendering.
2. [x] T2 (GREEN): implement selector option snapshot support in UI and gateway session discovery.
3. [x] T3 (REGRESSION): run existing chat/send marker suites to ensure no contract regression.
4. [x] T4 (VERIFY): run fmt/clippy/tests/guardrails/mutation and set spec status to `Implemented`.

## Tier Mapping
- Unit: session option normalization/discovery helper coverage (gateway).
- Property: N/A.
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: UI `/ops/chat` selector markers.
- Conformance: C-01..C-03.
- Integration: gateway `/ops/chat` selection sync with seeded sessions.
- Fuzz: N/A.
- Mutation: `cargo mutants --in-diff <diff-file> -p tau-dashboard-ui -p tau-gateway`.
- Regression: existing ops chat shell suites (`functional_spec_2830`).
- Performance: N/A.

## Verification Evidence
- Targeted:
  - `cargo test -p tau-dashboard-ui functional_spec_2834 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2834 -- --test-threads=1`
  - `cargo test -p tau-gateway integration_spec_2834 -- --test-threads=1`
- Regression:
  - `cargo test -p tau-dashboard-ui functional_spec_2830 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2830 -- --test-threads=1`
- Verify:
  - `cargo fmt --check`
  - `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
  - `python3 .github/scripts/oversized_file_guard.py`
  - `cargo mutants --in-diff /tmp/mutants_2834.diff -p tau-dashboard-ui -p tau-gateway` (`8/9` caught, `1` unviable compile-time mutant)
  - `cargo test -p tau-dashboard-ui`
  - `cargo test -p tau-gateway`
