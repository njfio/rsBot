# Tasks: Issue #2682 - PRD gateway audit summary and audit log endpoints

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-08.
2. [x] T2 (GREEN): add audit route constants/router wiring and `/gateway/status` discovery fields.
3. [x] T3 (GREEN): implement audit summary handler with merged source parsing + time-window filtering.
4. [x] T4 (GREEN): implement audit log handler with deterministic filtering + bounded pagination.
5. [x] T5 (REGRESSION): verify malformed-line handling and invalid query fail-closed behavior.
6. [x] T6 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-09 evidence.

## Tier Mapping
- Unit: helper-level parsing/filtering/pagination behavior in gateway audit runtime.
- Property: N/A (no randomized invariant requirement in this slice).
- Contract/DbC: N/A (contracts macros not used in touched modules).
- Snapshot: N/A (explicit field assertions used).
- Functional: C-01, C-02, C-04, C-05.
- Conformance: C-01..C-09.
- Integration: C-01, C-02, C-04, C-05, C-08.
- Fuzz: N/A (no new parser surface exposed to untrusted binary format).
- Mutation: N/A (bounded additive endpoint slice).
- Regression: C-03, C-06, C-07, C-08.
- Performance: N/A (no hotspot budget targeted in this slice).
