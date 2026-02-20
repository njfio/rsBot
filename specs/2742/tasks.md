# Tasks: Issue #2742 - G18 priority pages baseline in embedded dashboard shell

## Ordered Tasks
1. [x] T1 (RED): add failing shell tests for overview/sessions/memory/configuration controls and markers.
2. [x] T2 (GREEN): implement overview API wiring + deterministic render outputs.
3. [x] T3 (GREEN): implement sessions/memory/configuration API wiring + deterministic render outputs.
4. [x] T4 (REGRESSION): validate existing webchat/dashboard tests remain green.
5. [x] T5 (VERIFY): run fmt/clippy/tau-gateway tests and localhost live smoke.
6. [x] T6 (DOC): update G18 priority pages checklist evidence in `tasks/spacebot-comparison.md`.

## Tier Mapping
- Unit: C-01
- Property: N/A (no randomized invariants introduced)
- Contract/DbC: N/A (no contracts macro changes)
- Snapshot: N/A (explicit markers/assertions)
- Functional: C-02
- Conformance: C-01..C-05
- Integration: C-03, C-04
- Fuzz: N/A (no new parser boundary)
- Mutation: N/A (UI wiring scope)
- Regression: C-04
- Performance: N/A (no benchmark hotspots changed)
