# Plan: Issue #3148 - /ops/training status and control contracts

## Approach
1. Add RED tests for missing `/ops/training` status/rollout/optimizer/action markers.
2. Implement deterministic training panel contract sections in `tau-dashboard-ui`.
3. Add gateway integration test asserting rendered training markers on `/ops/training`.
4. Re-run nearby regression (`spec_3144`) and quality gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: expanding training panel can regress visibility handling for other routes.
  - Mitigation: explicit hidden regression case C-05 + rerun `spec_3144`.
- Risk: contract drift between UI and gateway rendering.
  - Mitigation: C-04 gateway integration conformance test.

## Interfaces / Contracts
New deterministic markers:
- `#tau-ops-training-status`
- `#tau-ops-training-rollouts`
- `#tau-ops-training-optimizer`
- `#tau-ops-training-actions`

## ADR
No ADR required.
