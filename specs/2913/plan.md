# Plan: Issue #2913 - ops memory type-filter narrowing contracts

## Approach
1. Add RED UI tests for deterministic type-filter control marker contracts.
2. Add RED gateway integration tests seeding mixed-type memory entries and asserting type-filter narrowing.
3. Implement minimal snapshot/control plumbing for selected memory type and narrowed search results.
4. Run regression + verify gates (fmt/clippy/spec slices/mutation/live validation).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: marker-order changes can regress existing search/scope assertions.
  - Mitigation: preserve existing marker ordering and add additive attributes/inputs only.
- Risk: type filtering could be bypassed by default search options.
  - Mitigation: seed deterministic mixed-type fixtures and assert inclusion/exclusion by ID.
- Risk: route-level regression outside memory panel.
  - Mitigation: rerun established chat/session/memory regression slices.

## Interface / Contract Notes
- No new endpoints or protocols.
- Additive query/form contracts on existing `/ops/memory` route.
