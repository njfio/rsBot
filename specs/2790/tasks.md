# Tasks: Issue #2790 - PRD Phase 1C 14-item sidebar navigation and breadcrumb shell markers

## Ordered Tasks
1. [x] T1 (RED): add failing tests for 14-route sidebar markers and breadcrumb route markers.
2. [x] T2 (GREEN): implement static 14-item navigation contract in `tau-dashboard-ui` shell.
3. [x] T3 (GREEN): add breadcrumb render contract keyed by active route context.
4. [x] T4 (REGRESSION): verify auth shell markers and gateway `/ops` + `/ops/login` route behavior remain stable.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests and mark spec `Implemented`.

## Tier Mapping
- Unit: `tau-dashboard-ui` nav/breadcrumb marker tests.
- Property: N/A (no randomized invariant behavior).
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: breadcrumb and navigation marker tests.
- Conformance: C-01..C-05.
- Integration: gateway `/ops` and `/ops/login` shell endpoint checks.
- Fuzz: N/A.
- Mutation: N/A (UI shell route marker slice).
- Regression: phase-1B auth marker checks and gateway route tests.
- Performance: N/A.
