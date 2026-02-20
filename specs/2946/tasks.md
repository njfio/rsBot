# Tasks: Issue #2946 - Accessibility contract markers and conformance tests

## Ordered Tasks
1. [x] T1 (RED): add failing accessibility conformance tests for C-01..C-05.
2. [x] T2 (GREEN): implement accessibility contract markers in shell output.
3. [x] T3 (REGRESSION): validate existing dashboard contract tests remain green.
4. [x] T4 (VERIFY): run fmt, clippy, and scoped tests; set spec status Implemented.

## Tier Mapping
- Unit: marker assertions.
- Property: N/A (declarative markers only).
- Contract/DbC: N/A (no contracts macro in this slice).
- Snapshot: N/A (explicit assertions preferred).
- Functional: accessibility marker surface checks.
- Conformance: C-01..C-05.
- Integration: N/A (crate-local SSR contract).
- Fuzz: N/A (no parser/untrusted input path).
- Mutation: N/A (UI marker contract slice).
- Regression: existing dashboard suite.
- Performance: N/A (no runtime path change).
