# Tasks: Issue #2670 - PRD channel lifecycle action gateway endpoint

## Ordered Tasks
1. [x] T1 (RED): add failing integration/regression tests for C-01..C-06.
2. [x] T2 (GREEN): add route constant + router wiring + status payload discovery field.
3. [x] T3 (GREEN): implement lifecycle handler with auth enforcement, validation, action execution, and deterministic JSON response.
4. [x] T4 (REFACTOR/REGRESSION): add helper parsers/bounds normalization for channel/action/probe config and ensure no behavior regression in existing status/webchat flows.
5. [x] T5 (VERIFY): run scoped fmt/clippy/targeted tests and capture C-07 evidence.

## Tier Mapping
- Unit: parser/helper validation tests embedded in gateway module tests.
- Property: N/A (no randomized invariant surface in this slice).
- Contract/DbC: N/A (contracts macros not used in touched module).
- Snapshot: N/A (assertions use explicit response fields/codes).
- Functional: C-01, C-02.
- Conformance: C-01..C-07.
- Integration: C-01..C-06.
- Fuzz: N/A (no new parser over untrusted binary format).
- Mutation: N/A (bounded endpoint wiring and validation logic, non-critical algorithm path).
- Regression: C-03, C-04, C-05, C-06.
- Performance: N/A (no new PRD performance budget targeted by this slice).
