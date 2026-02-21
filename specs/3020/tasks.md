# Tasks: Issue #3020 - Docs + archive workflow refresh

## Ordered Tasks
1. [x] T1 (RED): add docs/archive conformance tests and capture failing output before updates.
2. [x] T2 (GREEN): refresh README/operator/API docs with current capability and 70+ route coverage markers.
3. [x] T3 (GREEN): add implemented-spec archive script + archive operations guide and make tests pass.
4. [x] T4 (REGRESSION): rerun conformance tests and targeted command checks.
5. [x] T5 (VERIFY): run `cargo fmt --check` and `cargo check -q`.

## Tier Mapping
- Unit: shell assertion helpers for docs/archive scripts
- Property: N/A (no algorithmic invariants introduced)
- Contract/DbC: N/A (no API contract code changes)
- Snapshot: N/A (no snapshot tests)
- Functional: docs/archive conformance scripts
- Conformance: C-01, C-02, C-03, C-04, C-05, C-06
- Integration: N/A (docs/script workflow only)
- Fuzz: N/A (no fuzz harness changes)
- Mutation: N/A (docs/script workflow)
- Regression: conformance reruns after updates
- Performance: N/A (no runtime perf changes)
