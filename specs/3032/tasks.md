# Tasks: Issue #3032 - `/auth rotate-key` command

## Ordered Tasks
1. [x] T1 (RED): add parser/runtime/help tests for rotate-key and capture failing output.
2. [x] T2 (GREEN): implement rotate-key parser and execution path in auth runtime.
3. [x] T3 (GREEN): update command catalog help usage/example.
4. [x] T4 (REGRESSION): rerun targeted auth/help tests.
5. [x] T5 (VERIFY): run `cargo fmt --check` and `cargo check -q`.

## Tier Mapping
- Unit: parser and auth command execution tests
- Property: N/A (no randomized invariants)
- Contract/DbC: N/A (no contracts crate changes)
- Snapshot: N/A (no snapshot tests)
- Functional: end-to-end auth rotate-key command tests
- Conformance: C-01..C-06
- Integration: N/A (no cross-service transport change)
- Fuzz: N/A (no untrusted parser runtime)
- Mutation: N/A (non-critical command slice)
- Regression: targeted test reruns + baseline checks
- Performance: N/A (no perf-sensitive path change)
