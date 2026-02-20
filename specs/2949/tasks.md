# Tasks: Issue #2949 - Performance budget contract markers and conformance tests

## Ordered Tasks
1. [x] T1 (RED): add failing performance conformance tests for C-01..C-04.
2. [x] T2 (GREEN): implement performance budget marker contracts in shell output.
3. [x] T3 (REGRESSION): verify existing dashboard contract tests remain green.
4. [x] T4 (VERIFY): run fmt, clippy, and scoped tests; mark spec Implemented.

## Tier Mapping
- Unit: marker assertions.
- Property: N/A (declarative markers only).
- Contract/DbC: N/A (no contracts macro in this slice).
- Snapshot: N/A (explicit assertions preferred).
- Functional: performance budget marker checks.
- Conformance: C-01..C-04.
- Integration: N/A (crate-local SSR contract).
- Fuzz: N/A (no parser/untrusted input path).
- Mutation: N/A (UI contract marker slice).
- Regression: existing dashboard suite.
- Performance: N/A (this slice defines budgets as contracts; does not execute benchmarks).
