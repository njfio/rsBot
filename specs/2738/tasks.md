# Tasks: Issue #2738 - G18 embedded dashboard shell route

## Ordered Tasks
1. [x] T1 (RED): add failing tests for dashboard shell renderer + `/dashboard` endpoint + status advertisement.
2. [x] T2 (GREEN): add embedded dashboard shell renderer and deterministic markers.
3. [x] T3 (GREEN): wire `/dashboard` route and status payload endpoint field.
4. [x] T4 (REGRESSION): verify existing webchat/dashboard endpoint regressions remain green.
5. [x] T5 (VERIFY): run scoped fmt/clippy/tau-gateway tests and live localhost smoke for `/dashboard`.
6. [x] T6 (DOC): update G18 checklist evidence in `tasks/spacebot-comparison.md`.

## Tier Mapping
- Unit: C-01
- Property: N/A (no randomized invariants introduced)
- Contract/DbC: N/A (no contracts macro changes)
- Snapshot: N/A (marker assertions are explicit)
- Functional: C-02
- Conformance: C-01..C-05
- Integration: C-03, C-04
- Fuzz: N/A (no new untrusted parser boundary)
- Mutation: N/A (UI shell wiring, non-critical mutation lane)
- Regression: C-04
- Performance: N/A (no benchmark/hotspot change)
