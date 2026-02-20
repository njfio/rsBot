# Tasks: Issue #2704 - Cortex observer status endpoint and event tracking

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-05.
2. [x] T2 (GREEN): implement Cortex observer event persistence and status read model.
3. [x] T3 (GREEN): wire `GET /cortex/status` route and status discovery metadata.
4. [x] T4 (GREEN): hook observer event recording to selected gateway operations.
5. [x] T5 (REGRESSION): verify unauthorized and missing-artifact fallback behavior.
6. [x] T6 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-06 evidence.

## Tier Mapping
- Unit: observer helper parsing/rendering behavior coverage.
- Property: N/A (no randomized invariant algorithm introduced).
- Contract/DbC: N/A (contracts macros not introduced in touched modules).
- Snapshot: N/A (explicit field assertions used).
- Functional: C-01, C-02.
- Conformance: C-01..C-06.
- Integration: C-01, C-02, C-05.
- Fuzz: N/A (no new parser/codec boundary requiring fuzz harness in this bounded slice).
- Mutation: N/A (bounded additive endpoint slice).
- Regression: C-03, C-04, C-05.
- Performance: N/A (no hotspot/perf budget contract changed).
