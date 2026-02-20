# Plan: Issue #2790 - PRD Phase 1C 14-item sidebar navigation and breadcrumb shell markers

## Approach
1. Add RED tests for missing 14-route navigation and breadcrumb markers in `tau-dashboard-ui` and gateway route responses.
2. Expand `tau-dashboard-ui` shell renderer with a static 14-route navigation model and route-aware breadcrumb contract.
3. Ensure auth shell markers from phase-1B remain unchanged.
4. Re-run gateway integration tests for `/ops` and `/ops/login` and scoped quality gates.

## Affected Modules
- `specs/milestones/m131/index.md` (new)
- `specs/2790/spec.md` (new)
- `specs/2790/plan.md` (new)
- `specs/2790/tasks.md` (new)
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: route naming drift between PRD and shell nav links.
  - Mitigation: define one navigation constant list in `tau-dashboard-ui` and test exact paths.
- Risk: nav expansion could accidentally remove auth shell markers.
  - Mitigation: keep existing phase-1B assertions and add explicit regression checks.

## Interface and Contract Notes
- No new API endpoints.
- `tau-dashboard-ui` route enum and shell output contract extended with nav item ids and breadcrumb markers.
