# Plan: Issue #2794 - PRD Phase 1D gateway route coverage for 14 ops sidebar destinations

## Approach
1. Add RED integration tests that iterate all 14 sidebar route paths and assert current 404 failures.
2. Extend `tau-dashboard-ui` route context enum to represent the 14 ops views and map each to breadcrumb token/label.
3. Add gateway route constants/handlers for each path and render with explicit route context.
4. Re-run conformance tests plus phase-1B/1C regressions.
5. Run scoped fmt/clippy/tests and set spec to `Implemented`.

## Affected Modules
- `specs/milestones/m132/index.md` (new)
- `specs/2794/spec.md` (new)
- `specs/2794/plan.md` (new)
- `specs/2794/tasks.md` (new)
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: route constant mismatch between UI link hrefs and gateway router paths.
  - Mitigation: define explicit route constants in gateway matching UI hrefs and verify by table-driven test.
- Risk: expanding route enum may regress existing `/ops` and `/ops/login` markers.
  - Mitigation: keep existing spec_2786/spec_2790 tests and run them as regressions.

## Interface and Contract Notes
- No new API surface beyond route registrations.
- `TauOpsDashboardRoute` will expand from `Ops/Login` to route-specific variants for PRD views.
