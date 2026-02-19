# Tasks: Issue #2676 - PRD gateway safety policy GET/PUT endpoint

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-05.
2. [x] T2 (GREEN): add route constant/wiring and status discovery field.
3. [x] T3 (GREEN): implement `GET /gateway/safety/policy` with persisted/default source behavior.
4. [x] T4 (GREEN): implement `PUT /gateway/safety/policy` with validation + persistence.
5. [x] T5 (REGRESSION): verify unauthorized and invalid payload failures remain fail-closed.
6. [x] T6 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-06 evidence.

## Tier Mapping
- Unit: validation branches for policy payload checks.
- Property: N/A (no randomized invariant surface in this slice).
- Contract/DbC: N/A (contracts macros not used in touched module).
- Snapshot: N/A (assertions use explicit response fields/codes).
- Functional: C-01, C-02.
- Conformance: C-01..C-06.
- Integration: C-01, C-02, C-05.
- Fuzz: N/A (no new untrusted binary parser path).
- Mutation: N/A (bounded endpoint/persistence behavior, non-critical algorithm path).
- Regression: C-03, C-04, C-05.
- Performance: N/A (no performance budget contract targeted in this slice).
