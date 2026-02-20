# Plan: Issue #2939 - Deploy Agent wizard panel and conformance tests

## Approach
1. Add RED conformance tests in `crates/tau-dashboard-ui/src/lib.rs` for deploy-route markers and non-deploy regression.
2. Refactor shell rendering to support route-specific sections via a new render entrypoint accepting route input.
3. Render deploy panel contract markers only when route equals `/ops/deploy`.
4. Keep existing foundation marker behavior unchanged for baseline shell route.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `specs/milestones/m165/index.md`
- `specs/2939/spec.md`
- `specs/2939/tasks.md`

## Risks / Mitigations
- Risk: breaking existing callers of `render_tau_ops_dashboard_shell`.
  - Mitigation: retain existing function as compatibility wrapper and add route-aware helper.
- Risk: overfitting to checklist text instead of stable markers.
  - Mitigation: use deterministic `id`/`data-*` markers and map tests to conformance IDs.

## Interfaces / Contracts
- Keep `pub fn render_tau_ops_dashboard_shell() -> String`.
- Add `pub fn render_tau_ops_dashboard_shell_for_route(route: &str) -> String` for route-specific rendering.

## ADR
No ADR required for this task: no new dependency, protocol, or architectural boundary change.
