# Tasks: Issue #2679 - PRD gateway safety rules and safety test endpoints

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-07.
2. [x] T2 (GREEN): add `tau-safety` rule contracts, defaults projection, validation, and scan helpers.
3. [x] T3 (GREEN): re-export safety-rule contracts/functions from `tau-agent-core`.
4. [x] T4 (GREEN): add safety rules/test route constants, router wiring, and status discovery fields.
5. [x] T5 (GREEN): implement `GET/PUT /gateway/safety/rules` with validation + persistence.
6. [x] T6 (GREEN): implement `POST /gateway/safety/test` using active policy + rule-set evaluation.
7. [x] T7 (REGRESSION): verify unauthorized/invalid request paths remain fail-closed.
8. [x] T8 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-08 evidence.

## Tier Mapping
- Unit: rule validation and scan helper behavior.
- Property: N/A (no randomized property requirement in this slice).
- Contract/DbC: N/A (contracts macros not used in touched modules).
- Snapshot: N/A (explicit field assertions used).
- Functional: C-01, C-02, C-04.
- Conformance: C-01..C-08.
- Integration: C-01, C-02, C-04, C-07.
- Fuzz: N/A (no new parser requiring fuzz gate in this slice).
- Mutation: N/A (bounded endpoint/rule-evaluation API slice).
- Regression: C-03, C-05, C-06, C-07.
- Performance: N/A (no hotspot budget targeted).
