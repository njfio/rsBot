# Tasks: Issue #2673 - PRD gateway config GET/PATCH endpoints

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-06.
2. [x] T2 (GREEN): add `/gateway/config` route constant/wiring and status discovery field.
3. [x] T3 (GREEN): implement `GET /gateway/config` active snapshot + pending overrides response.
4. [x] T4 (GREEN): implement `PATCH /gateway/config` validation + pending override writes + heartbeat hot-reload policy update.
5. [x] T5 (REGRESSION): ensure invalid payloads and unauthorized calls remain fail-closed.
6. [x] T6 (VERIFY): run scoped fmt/clippy/targeted tests and record C-07 evidence.

## Tier Mapping
- Unit: parser/validation checks in gateway tests for invalid payload branches.
- Property: N/A (no randomized invariant surface in this slice).
- Contract/DbC: N/A (contracts macros not used in touched module).
- Snapshot: N/A (assertions use explicit response fields/status/error codes).
- Functional: C-01, C-02.
- Conformance: C-01..C-07.
- Integration: C-01, C-02, C-06.
- Fuzz: N/A (no new untrusted binary parser path).
- Mutation: N/A (bounded endpoint behavior, non-critical algorithm path).
- Regression: C-03, C-04, C-05, C-06.
- Performance: N/A (no performance budget contract targeted in this slice).
