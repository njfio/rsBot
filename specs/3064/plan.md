# Plan: Issue #3064 - ops memory detail embedding and relations contracts

## Approach
1. Add RED UI tests for detail-panel markers and default hidden-state contracts.
2. Add RED gateway integration tests for selected-memory embedding metadata and
   relation row rendering.
3. Implement minimal selected-memory detail flow and deterministic SSR markers.
4. Run regression and verification gates for existing memory slices.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: detail selection parameters may drift from existing search controls.
  - Mitigation: deterministic query marker names and route-level integration tests.
- Risk: embedding metadata may be absent on legacy records.
  - Mitigation: default/empty markers with explicit dimension/reason fallbacks.
- Risk: regression in existing memory create/edit/delete contracts.
  - Mitigation: rerun existing memory spec suites as regression gate.

## Interface / Contract Notes
- `/ops/memory` query controls gain selected-detail key marker support.
- No external API additions; behavior remains within ops-shell route rendering.
