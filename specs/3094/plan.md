# Plan: Issue #3094 - ops memory-graph zoom in/out contracts

## Approach
1. Add RED UI tests for default zoom contracts and clamped zoom-level action links.
2. Add RED gateway integration tests for query-driven zoom contract rendering.
3. Implement zoom query normalization and deterministic zoom control markers.
4. Run regression suites and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: introducing query-driven zoom may regress existing route-state composition.
  - Mitigation: centralize zoom normalization in controls query and keep defaults deterministic.
- Risk: zoom links could drop existing session/scope selectors.
  - Mitigation: derive links from existing route context markers and assert full hrefs in tests.

## Interface / Contract Notes
- Extend controls query with `graph_zoom` parsing (clamped to `[0.25, 2.00]`).
- Extend dashboard snapshot with `memory_graph_zoom_level` string contract.
- Add zoom markers/links:
  - `#tau-ops-memory-graph-zoom-controls`
  - `#tau-ops-memory-graph-zoom-in`
  - `#tau-ops-memory-graph-zoom-out`
- P1 process rule: spec marked Reviewed; human review requested in PR.
