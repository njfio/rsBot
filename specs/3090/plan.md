# Plan: Issue #3090 - ops memory-graph hover highlight contracts

## Approach
1. Add RED UI tests for node neighbor and edge highlight marker contracts.
2. Add RED gateway integration tests for focused-memory highlight mappings.
3. Implement deterministic focus-to-highlight marker derivation in memory graph rendering.
4. Run regression suites and verification gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: focus contracts may drift from detail-panel selected memory semantics.
  - Mitigation: derive highlight markers from existing selected detail memory ID state.
- Risk: new marker attributes could regress prior graph contracts.
  - Mitigation: rerun `spec_3086`, `spec_3082`, `spec_3078`, and baseline memory suites.

## Interface / Contract Notes
- Extend node rows with `data-node-hover-neighbor` markers.
- Extend edge rows with `data-edge-hover-highlighted` markers.
- No external API changes; SSR marker-level contracts only.
- P1 process rule: spec marked Reviewed; human review requested in PR.
