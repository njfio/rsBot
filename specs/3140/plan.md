# Plan: Issue #3140 - ops route panel contracts for config/training/safety/diagnostics

## Approach
1. Add RED tests for missing route panel contracts in `tau-dashboard-ui` and gateway integration tests.
2. Implement dedicated panel sections and deterministic route markers in the shell renderer.
3. Add deterministic endpoint template markers per panel for config/training/safety/diagnostics.
4. Re-run nearby regressions to ensure tools/channels/chat/session contracts remain stable.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: introducing new route panels could break hidden/visible behavior for existing routes.
  - Mitigation: add explicit regression test for `/ops` route panel visibility state.
- Risk: gateway shell route output diverges from UI contracts.
  - Mitigation: add route-specific integration tests in gateway module.

## Interfaces / Contracts
New deterministic panel markers:
- `#tau-ops-config-panel`
- `#tau-ops-training-panel`
- `#tau-ops-safety-panel`
- `#tau-ops-diagnostics-panel`

## ADR
No ADR required.
