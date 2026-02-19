# Tasks: Issue #2697 - PRD gateway deploy and stop agent endpoints

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-06.
2. [x] T2 (GREEN): add deploy runtime module and endpoint route/status discovery wiring.
3. [x] T3 (GREEN): implement deploy/stop handlers with deterministic persisted state and error mapping.
4. [x] T4 (REGRESSION): verify unknown-id stop, invalid deploy input, and unauthorized behavior.
5. [x] T5 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-07 evidence.

## Tier Mapping
- Unit: runtime helper/state mapping behavior coverage.
- Property: N/A (no randomized invariant algorithm introduced).
- Contract/DbC: N/A (contracts macros not introduced in touched modules).
- Snapshot: N/A (explicit field assertions used).
- Functional: C-01, C-02.
- Conformance: C-01..C-07.
- Integration: C-01, C-02, C-05.
- Fuzz: N/A (no new parser/codec boundary requiring fuzz harness in this bounded slice).
- Mutation: N/A (bounded additive endpoint slice).
- Regression: C-03, C-04, C-05, C-06.
- Performance: N/A (no hotspot/perf budget contract changed).
