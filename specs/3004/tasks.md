# Tasks: Issue #3004 - Refresh Tau gaps revalidation doc

## Ordered Tasks
1. [ ] T1 (RED): add doc conformance script and run it against current doc to capture stale-status failure.
2. [ ] T2 (GREEN): update `tasks/tau-gaps-issues-improvements.md` with current closure states and refreshed snapshot metadata.
3. [ ] T3 (REGRESSION): re-run conformance script to ensure refreshed content remains enforced.
4. [ ] T4 (VERIFY): run baseline quality gates for this slice (`cargo fmt --check`, `cargo check -q`).

## Tier Mapping
- Unit: script assertion helpers in `test-tau-gaps-issues-improvements.sh`
- Property: N/A (no algorithmic invariant logic change)
- Contract/DbC: N/A (docs + shell validation only)
- Snapshot: N/A (no insta snapshots)
- Functional: documentation closure-state correctness checks
- Conformance: C-01, C-02, C-03, C-04
- Integration: N/A (no cross-service runtime integration changes)
- Fuzz: N/A (no parser/untrusted input surface change)
- Mutation: N/A (documentation+shell assertions)
- Regression: conformance script rerun after doc refresh
- Performance: N/A (no runtime perf surface change)
