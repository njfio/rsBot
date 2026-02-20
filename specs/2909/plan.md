# Plan: Issue #2909 - ops memory scope-filter narrowing contracts

## Approach
1. Add RED UI tests for deterministic workspace/channel/actor filter marker contracts and value preservation.
2. Add RED gateway integration tests seeding mixed-scope memory entries and asserting narrowed result sets.
3. Implement minimal scope-filter snapshot fields and memory-panel form control rendering.
4. Run regression + verify gates (fmt/clippy/spec slices/mutation/live validation).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: additive filter controls could break deterministic HTML contract assertions.
  - Mitigation: keep stable IDs/attributes and assert exact marker contracts in tests.
- Risk: scope-filter matching could drift under memory ranking behavior.
  - Mitigation: seed deterministic cross-scope fixtures with exact summary tokens and assert inclusion/exclusion by `memory_id`.
- Risk: regression impact on existing ops routes.
  - Mitigation: rerun memory/chat/session regression slice before PR.

## Interface / Contract Notes
- No new endpoints.
- No wire-format changes.
- Additive query/form contracts for existing `/ops/memory` route.
