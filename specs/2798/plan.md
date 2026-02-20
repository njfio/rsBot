# Plan: Issue #2798 - PRD Phase 1E responsive sidebar and theme shell controls

## Approach
1. Add RED tests for missing responsive sidebar and theme marker contracts in `tau-dashboard-ui` and gateway shell output.
2. Expand `TauOpsDashboardShellContext` with explicit `theme` and `sidebar_state` signaling enums.
3. Add responsive + theme control markup/markers to SSR shell while preserving existing IDs and route/auth markers.
4. Re-run phase-1B/1C/1D regressions to ensure no contract drift.
5. Run scoped fmt/clippy/tests and set spec status to `Implemented`.

## Affected Modules
- `specs/milestones/m133/index.md` (new)
- `specs/2798/spec.md` (new)
- `specs/2798/plan.md` (new)
- `specs/2798/tasks.md` (new)
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: new shell markers accidentally alter existing route/auth markers.
  - Mitigation: retain prior IDs/attributes and run phase-1B/1C/1D regressions.
- Risk: expanding shell context could break gateway shell rendering call sites.
  - Mitigation: use context defaults/explicit construction and compile-time checks.

## Interface and Contract Notes
- Extend `tau-dashboard-ui` public shell context with explicit theme/sidebar state enums.
- No new gateway endpoints or protocol changes.
