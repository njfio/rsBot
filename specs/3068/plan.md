# Plan: Issue #3068 - ops memory-graph nodes and edges contracts

## Approach
1. Add RED UI tests for `/ops/memory-graph` panel/list marker defaults and hidden-state behavior.
2. Add RED gateway integration tests for graph node/edge hydration from related memory records.
3. Implement minimal memory-graph context rows and SSR markers in dashboard shell.
4. Run regression slices for existing memory route specs and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: graph rows may be non-deterministic due to record ordering.
  - Mitigation: deterministic sort for nodes/edges by stable keys.
- Risk: relation edges may reference nodes outside current slice.
  - Mitigation: include only edges whose source+target are present in rendered node set.
- Risk: regression in existing memory route contracts.
  - Mitigation: rerun prior memory spec suites as required regression gate.

## Interface / Contract Notes
- No external API additions.
- Contract additions are limited to SSR HTML markers under `/ops/memory-graph`.
- P1 process rule: spec marked Reviewed; human review requested in PR.
