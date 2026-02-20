# Tasks: Issue #2708 - Cortex observer coverage for memory-save and worker-progress signals

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-05.
2. [x] T2 (GREEN): add Cortex runtime helper wrappers for new event types.
3. [x] T3 (GREEN): instrument memory write/update/delete handlers.
4. [x] T4 (GREEN): instrument external coding progress/followup handlers.
5. [x] T5 (REGRESSION): verify auth + missing-artifact fallback contracts remain intact.
6. [x] T6 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-06 evidence.

## Tier Mapping
- Unit: helper wrapper behavior and status loader expectations.
- Property: N/A (no new invariant-heavy randomized algorithm introduced).
- Contract/DbC: N/A (no contracts macro surface introduced).
- Snapshot: N/A (behavior asserted via explicit field checks).
- Functional: C-01, C-02, C-03.
- Conformance: C-01..C-06.
- Integration: C-01, C-02, C-03.
- Fuzz: N/A (no new untrusted parser boundary introduced in this slice).
- Mutation: N/A (bounded additive instrumentation slice).
- Regression: C-04, C-05.
- Performance: N/A (no hotspot/perf budget contract changed).
