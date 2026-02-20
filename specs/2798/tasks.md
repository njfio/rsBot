# Tasks: Issue #2798 - PRD Phase 1E responsive sidebar and theme shell controls

## Ordered Tasks
1. [x] T1 (RED): add failing shell contract tests for responsive sidebar + theme markers.
2. [x] T2 (GREEN): expand shell context with theme/sidebar state and add responsive/theme control markup.
3. [x] T3 (GREEN): add gateway integration assertion for phase-1E contract markers on `/ops`.
4. [x] T4 (REGRESSION): run phase-1B/1C/1D regression tests.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests and set spec status to `Implemented`.

## Tier Mapping
- Unit: shell context/theme/sidebar marker tests in `tau-dashboard-ui`.
- Property: N/A (no randomized invariant domain).
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: responsive/theme shell marker assertions.
- Conformance: C-01..C-05.
- Integration: gateway `/ops` shell output marker coverage.
- Fuzz: N/A.
- Mutation: N/A (SSR marker contract slice).
- Regression: phase-1B/1C/1D route/auth tests.
- Performance: N/A.
