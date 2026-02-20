# Tasks: Issue #2794 - PRD Phase 1D gateway route coverage for 14 ops sidebar destinations

## Ordered Tasks
1. [x] T1 (RED): add failing integration tests for all 14 `/ops/*` route destinations.
2. [x] T2 (GREEN): expand dashboard route context mapping in `tau-dashboard-ui` for 14 views.
3. [x] T3 (GREEN): wire gateway route constants/handlers for all sidebar destinations with context-aware rendering.
4. [x] T4 (REGRESSION): run phase-1B/1C route/auth regression tests.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests and set spec status to `Implemented`.

## Tier Mapping
- Unit: route-context/breadcrumb mapping behavior in `tau-dashboard-ui` tests.
- Property: N/A (no randomized invariants).
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: route marker assertions in UI and gateway tests.
- Conformance: C-01..C-05.
- Integration: table-driven 14-route gateway response coverage.
- Fuzz: N/A.
- Mutation: N/A (route wiring + marker slice).
- Regression: phase-1B/1C and legacy dashboard/auth checks.
- Performance: N/A.
