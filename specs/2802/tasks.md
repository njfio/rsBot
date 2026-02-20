# Tasks: Issue #2802 - Query-driven shell control behavior for theme and sidebar state

## Ordered Tasks
1. [x] T1 (RED): add failing gateway tests for valid and invalid `theme`/`sidebar` query behavior.
2. [x] T2 (GREEN): add parser helper module and integrate query state into ops shell handlers.
3. [x] T3 (REGRESSION): run phase-1B/1C/1D/1E regression tests.
4. [x] T4 (VERIFY): run fmt/clippy/scoped tests and set spec status to `Implemented`.

## Tier Mapping
- Unit: parser normalization behavior (if covered in module tests).
- Property: N/A (finite enum-like query domain).
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: query-driven marker assertions.
- Conformance: C-01..C-04.
- Integration: `/ops*` route requests with query combinations.
- Fuzz: N/A.
- Mutation: N/A (handler/query wiring slice).
- Regression: phase-1B/1C/1D/1E suites.
- Performance: N/A.
