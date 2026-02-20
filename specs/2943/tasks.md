# Tasks: Issue #2943 - Real-time stream connection markers and conformance tests

## Ordered Tasks
1. [x] T1 (RED): add failing stream conformance tests for C-01..C-06.
2. [x] T2 (GREEN): implement stream contract marker section in shell render output.
3. [x] T3 (REGRESSION): ensure existing route/panel marker tests remain green.
4. [x] T4 (VERIFY): run fmt, clippy, and scoped tests; set spec status to Implemented.

## Tier Mapping
- Unit: marker presence assertions.
- Property: N/A (static contract markers).
- Contract/DbC: N/A (no contracts macro in this slice).
- Snapshot: N/A (explicit marker assertions).
- Functional: stream contract marker presence.
- Conformance: C-01..C-06.
- Integration: N/A (crate-local SSR contract).
- Fuzz: N/A (no parser/untrusted input path).
- Mutation: N/A (UI contract scaffold).
- Regression: existing dashboard tests remain green.
- Performance: N/A (no runtime execution path changes).
