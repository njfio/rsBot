# Plan: Issue #3103 - ops memory-graph filter contracts

## Approach
1. Add RED UI tests for default filter contracts and query-driven filtered graph outputs.
2. Add RED gateway integration tests for query-driven filter contract rendering.
3. Implement filter query normalization and deterministic filter marker/action rendering.
4. Apply filters to graph node/edge contract views.
5. Run regression suites and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: filter links may drop session/search/zoom/pan state.
  - Mitigation: derive links from shared route base and assert query fragments in tests.
- Risk: filtering edges without filtering node scope can produce inconsistent graph contracts.
  - Mitigation: derive allowed node ids first and filter edges against both selected relation type and node scope.

## Interface / Contract Notes
- Extend controls query with:
  - `graph_filter_memory_type`
  - `graph_filter_relation_type`
- Extend dashboard snapshot with:
  - `memory_graph_filter_memory_type`
  - `memory_graph_filter_relation_type`
- Add filter markers/links:
  - `#tau-ops-memory-graph-filter-controls`
  - `#tau-ops-memory-graph-filter-memory-type-all`
  - `#tau-ops-memory-graph-filter-memory-type-goal`
  - `#tau-ops-memory-graph-filter-relation-type-all`
  - `#tau-ops-memory-graph-filter-relation-type-related-to`
- P1 process rule: spec marked Reviewed; human review requested in PR.
