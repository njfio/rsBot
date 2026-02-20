# Tasks: Issue #2806 - Command-center live-data SSR markers from dashboard snapshot

## Ordered Tasks
1. [x] T1 (RED): add failing `/ops` shell integration tests for health/KPI/alerts/timeline live-data markers.
2. [x] T2 (GREEN): extend `tau-dashboard-ui` shell context/markup for command-center snapshot markers.
3. [x] T3 (GREEN): map gateway dashboard snapshot data into shell context.
4. [x] T4 (REGRESSION): run phase-1B/1C/1D/1E/1F regression tests.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests and set spec status to `Implemented`.

## Tier Mapping
- Unit: ui render assertions for command-center snapshot markers.
- Property: N/A (deterministic mappings only).
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: command-center marker assertions.
- Conformance: C-01..C-04.
- Integration: `/ops` route render with dashboard snapshot fixture.
- Fuzz: N/A.
- Mutation: `cargo mutants --in-diff /tmp/mutants_2806.diff -p tau-gateway -p tau-dashboard-ui` (10 tested, 4 caught, 6 unviable, 0 escaped).
- Regression: phase-1B/1C/1D/1E/1F suites.
- Performance: N/A.
