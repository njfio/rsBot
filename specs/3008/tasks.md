# Tasks: Issue #3008 - tau-diagnostics boundary tests

## Ordered Tasks
1. [x] T1 (RED): add parser and audit-boundary conformance tests in `tau-diagnostics` and capture failing evidence.
2. [x] T2 (GREEN): apply minimal assertions/fixture adjustments so new tests pass without behavior regressions.
3. [x] T3 (REGRESSION): run targeted new tests and full crate tests.
4. [x] T4 (VERIFY): run baseline checks for touched scope (`cargo fmt --check`, `cargo check -q`).

## Tier Mapping
- Unit: parser boundary test(s)
- Property: N/A (no randomized invariant introduction in this slice)
- Contract/DbC: N/A (no DbC annotations in crate)
- Snapshot: N/A (no insta snapshots)
- Functional: audit summarization fixture behavior test(s)
- Conformance: C-01, C-02, C-03, C-04
- Integration: N/A (single crate test-only slice)
- Fuzz: N/A (no parser fuzz harness changes)
- Mutation: N/A (test-only uplift)
- Regression: malformed-json context test + full crate rerun
- Performance: N/A (no runtime behavior change)
