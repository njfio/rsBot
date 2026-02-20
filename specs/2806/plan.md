# Plan: Issue #2806 - Command-center live-data SSR markers from dashboard snapshot

## Approach
1. Add RED integration tests for `/ops` shell expecting live-data health/KPI/alerts/timeline markers from snapshot fixtures.
2. Extend `tau-dashboard-ui` shell context with command-center snapshot payload and render deterministic marker attributes/entries.
3. Map gateway dashboard snapshot into the new UI context payload for ops-shell rendering.
4. Re-run phase-1B/1C/1D/1E/1F regressions.
5. Run scoped fmt/clippy/tests and set spec status to `Implemented`.

## Affected Modules
- `specs/milestones/m135/index.md` (new)
- `specs/2806/spec.md` (new)
- `specs/2806/plan.md` (new)
- `specs/2806/tasks.md` (new)
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: large `gateway_openresponses.rs` regresses oversized-file guard.
  - Mitigation: keep mapping helper small and avoid expanding handler boilerplate.
- Risk: live-data markers could break prior static marker assertions.
  - Mitigation: preserve existing IDs/`data-component` markers and run full regression suites.

## Interface and Contract Notes
- Adds command-center snapshot payload to `TauOpsDashboardShellContext`.
- No new network endpoints or protocol changes.
