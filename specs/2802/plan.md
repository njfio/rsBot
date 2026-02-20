# Plan: Issue #2802 - Query-driven shell control behavior for theme and sidebar state

## Approach
1. Add RED gateway tests that request `/ops*` routes with control query params and assert missing behavior.
2. Introduce a small `gateway_openresponses` helper module that parses and normalizes `theme`/`sidebar` query values.
3. Wire parsed control state into `TauOpsDashboardShellContext` from route handlers.
4. Re-run phase-1B/1C/1D/1E regressions.
5. Run scoped fmt/clippy/tests and set spec status to `Implemented`.

## Affected Modules
- `specs/milestones/m134/index.md` (new)
- `specs/2802/spec.md` (new)
- `specs/2802/plan.md` (new)
- `specs/2802/tasks.md` (new)
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs` (new)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: gateway_openresponses oversized-file guard regression.
  - Mitigation: keep main file changes minimal and move parsing logic into a new submodule.
- Risk: query parsing regresses existing route handlers.
  - Mitigation: table-driven integration tests over multiple `/ops*` routes + prior regression suites.

## Interface and Contract Notes
- No new endpoints.
- Adds query extraction contract for existing `/ops*` handlers.
