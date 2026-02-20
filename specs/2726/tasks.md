# Tasks: Issue #2726 - G19 phase-2 API parity and force-layout rendering

## Ordered Tasks
1. [x] T1 (RED): add failing conformance/regression tests for `/api/memories/graph` and force-layout script expectations.
2. [x] T2 (GREEN): add `/api/memories/graph` route + shared handler logic with existing graph payload behavior.
3. [x] T3 (GREEN): refactor webchat memory graph rendering from ring layout to deterministic force-layout simulation.
4. [x] T4 (REGRESSION): verify existing `/gateway/memory-graph/{session_key}` and auth behavior remain stable.
5. [x] T5 (VERIFY): run scoped fmt/clippy/targeted gateway tests for C-06.
6. [x] T6 (DOC): update `tasks/spacebot-comparison.md` G19 checklist lines completed by this slice.

## Tier Mapping
- Unit: C-04
- Property: N/A (no randomized property harness required for bounded UI simulation)
- Contract/DbC: N/A (no contracts macro adoption in this slice)
- Snapshot: N/A (assert explicit payload/script markers)
- Functional: C-05
- Conformance: C-01..C-06
- Integration: C-01, C-03
- Fuzz: N/A (no untrusted parser boundary added)
- Mutation: N/A (gateway route/UI parity slice, non-critical mutation lane)
- Regression: C-02, C-04
- Performance: N/A (no hotspot SLA contract introduced)
