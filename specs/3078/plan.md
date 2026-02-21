# Plan: Issue #3078 - ops memory-graph node color type contracts

## Approach
1. Add RED UI tests for node-color marker contracts on memory graph rows.
2. Add RED gateway integration tests for multi-type node-color mapping.
3. Implement deterministic memory-type -> color-token/value mapping and marker rendering.
4. Run regression suites for memory graph/explorer contracts and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: introducing color mapping could drift across layers.
  - Mitigation: central deterministic mapping helper in UI contract rendering.
- Risk: marker additions could regress prior node-size contracts.
  - Mitigation: rerun `spec_3070` and memory-graph baseline suites.

## Interface / Contract Notes
- Extend memory graph node rows with `data-node-color-token` and
  `data-node-color-hex` markers.
- No external API additions; contracts are SSR marker-level only.
- P1 process rule: spec marked Reviewed; human review requested in PR.
