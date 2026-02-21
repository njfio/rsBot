# Plan: Issue #3128 - ops channels list health contracts

## Approach
1. Add RED conformance tests for `/ops/channels` panel/summary/rows in UI and gateway.
2. Add channels panel rendering to dashboard shell using existing connector health snapshot rows.
3. Add deterministic summary counters (online/offline/degraded) and row markers.
4. Run scoped regressions for existing connector health contracts.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: overlap with existing command-center connector table markers.
  - Mitigation: use separate `/ops/channels` ids and keep existing command-center markers unchanged.
- Risk: liveness classification ambiguity for summary counters.
  - Mitigation: deterministic mapping (`open/online -> online`, `unknown/offline -> offline`, everything else -> degraded).

## Interfaces / Contracts
- Added shell markers:
  - `#tau-ops-channels-panel`
  - `#tau-ops-channels-summary`
  - `#tau-ops-channels-table`
  - `#tau-ops-channels-row-<index>`

## ADR
No ADR required.
