# Tasks: Issue #2770 - G10 serenity dependency and tau-discord-runtime foundation

## Ordered Tasks
1. [ ] T1 (RED): add failing compile/regression checks for new crate wiring and preserved Discord behavior.
2. [ ] T2 (GREEN): add `serenity` dependency in approved minimal scope.
3. [ ] T3 (GREEN): scaffold and wire `tau-discord-runtime` crate/module boundary.
4. [ ] T4 (REGRESSION): run existing Discord/multi-channel regression suite and fix drift.
5. [ ] T5 (VERIFY): run fmt/clippy/tests and collect evidence.
6. [ ] T6 (DOC): update G10 checklist rows with `#2770` evidence.

## Tier Mapping
- Unit: manifest/path helper checks where applicable
- Property: N/A (no invariant algorithm changes expected)
- Contract/DbC: N/A (no contract macro changes expected)
- Snapshot: N/A
- Functional: compile/wiring and behavior checks
- Conformance: C-01..C-04
- Integration: C-02, C-03
- Fuzz: N/A
- Mutation: N/A (dependency/crate wiring slice)
- Regression: C-03
- Performance: N/A
