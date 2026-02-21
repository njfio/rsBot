# Plan: Issue #3132 - ops channels action contracts

## Approach
1. Add RED conformance tests for channel action markers and enabled-state contracts in UI and gateway.
2. Extend `/ops/channels` rows with deterministic action markers (`login`, `logout`, `probe`).
3. Encode deterministic enabled-state mapping from channel liveness.
4. Run scoped regressions for existing channels list contracts.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: action-state rules drift from expected operational behavior.
  - Mitigation: codify deterministic mapping in tests (open/online: logout+probe enabled, login disabled; offline/unknown: login+probe enabled, logout disabled).
- Risk: regressions in existing channels panel contracts.
  - Mitigation: rerun `spec_3128` regression suite.

## Interfaces / Contracts
- Added row markers:
  - `#tau-ops-channels-login-<index>`
  - `#tau-ops-channels-logout-<index>`
  - `#tau-ops-channels-probe-<index>`

## ADR
No ADR required.
