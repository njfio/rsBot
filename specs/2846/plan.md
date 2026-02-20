# Plan: Issue #2846 - /ops/sessions/{session_key} session graph node/edge contracts

## Approach
1. Extend `tau-dashboard-ui` session detail contracts with graph panel/list/node/edge/empty-state markers.
2. Extend gateway session detail snapshot builder to derive graph node/edge rows from `SessionStore::lineage_entries` parent links.
3. Keep existing route and marker contracts stable and verified via regression tests.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: marker ordering instability for nodes/edges.
  - Mitigation: derive rows from lineage order and deterministic parent->child mapping.
- Risk: regressions in existing session detail contracts.
  - Mitigation: rerun `spec_2842` and related regressions after implementation.

## Interface / Contract Notes
- Additive deterministic markers only; no route or transport contract removals.
