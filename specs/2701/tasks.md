# Tasks: Issue #2701 - Cortex admin chat SSE endpoint foundation

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-05.
2. [x] T2 (GREEN): add cortex runtime module with deterministic SSE contract and validation.
3. [x] T3 (GREEN): wire `/cortex/chat` route and gateway status discovery metadata.
4. [x] T4 (REGRESSION): verify unauthorized and invalid payload fail-closed behavior.
5. [x] T5 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-06 evidence.

## Tier Mapping
- Unit: helper/event payload behavior coverage.
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
