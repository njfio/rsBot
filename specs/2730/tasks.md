# Tasks: Issue #2730 - G18 stretch Cortex admin chat webchat panel

## Ordered Tasks
1. [x] T1 (RED): add failing webchat conformance tests for Cortex panel markup/script stream markers.
2. [x] T2 (GREEN): add Cortex admin tab/view controls and endpoint constant wiring.
3. [x] T3 (GREEN): implement Cortex prompt submit + SSE stream rendering/status handling.
4. [x] T4 (REGRESSION): verify existing webchat and cortex API tests remain green.
5. [x] T5 (VERIFY): run scoped fmt/clippy/targeted gateway tests.
6. [x] T6 (DOC): update `tasks/spacebot-comparison.md` G18 stretch evidence as applicable.

## Tier Mapping
- Unit: C-01, C-02
- Property: N/A (no randomized invariant harness introduced)
- Contract/DbC: N/A (no contracts macro changes)
- Snapshot: N/A (explicit marker assertions used)
- Functional: C-03
- Conformance: C-01..C-05
- Integration: C-03, C-04
- Fuzz: N/A (no untrusted parser boundary added)
- Mutation: N/A (UI parity slice, non-critical mutation lane)
- Regression: C-04
- Performance: N/A (no benchmark SLA introduced)
