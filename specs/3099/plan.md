# Plan: Issue #3099 - ops memory-graph pan contracts

## Approach
1. Add RED UI tests for default pan contracts and clamped directional pan links.
2. Add RED gateway integration tests for query-driven pan contract rendering.
3. Implement pan query normalization and deterministic pan control markers.
4. Run regression suites and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: pan links may drop existing route/session selector state.
  - Mitigation: derive links from existing route context and assert full `href` fragments in tests.
- Risk: clamp math or directional mapping may regress expected behavior.
  - Mitigation: centralize pan parsing/clamping in controls query and assert deterministic next states.

## Interface / Contract Notes
- Extend controls query with `graph_pan_x` and `graph_pan_y` parsing (clamped to `[-500.0, 500.0]`).
- Extend dashboard snapshot with `memory_graph_pan_x_level` and `memory_graph_pan_y_level` string contracts.
- Add pan markers/links:
  - `#tau-ops-memory-graph-pan-controls`
  - `#tau-ops-memory-graph-pan-left`
  - `#tau-ops-memory-graph-pan-right`
  - `#tau-ops-memory-graph-pan-up`
  - `#tau-ops-memory-graph-pan-down`
- P1 process rule: spec marked Reviewed; human review requested in PR.
