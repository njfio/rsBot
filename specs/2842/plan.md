# Plan: Issue #2842 - /ops/sessions/{session_key} detail timeline/validation/usage contracts

## Approach
1. Add a session-detail snapshot contract in `tau-dashboard-ui` and render deterministic SSR markers for panel, timeline, validation, and usage summaries.
2. Add gateway route wiring for `/ops/sessions/{session_key}` and collect selected-session detail data from `SessionStore` (`lineage_entries`, `validation_report`, `usage_summary`).
3. Keep existing `/ops/sessions` list + `/ops/chat` contract rendering unchanged; run regression coverage.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: SSR contract regressions on existing routes.
  - Mitigation: run existing `spec_2838` and `spec_2834` suites after implementation.
- Risk: detail route session-key mismatch between path and query selection.
  - Mitigation: path session key is sanitized and used as authoritative key for detail snapshot + chat/session selection context.
- Risk: floating-point formatting instability for cost marker.
  - Mitigation: normalize `estimated_cost_usd` marker string with fixed precision.

## Interface / Contract Notes
- New deterministic markers are additive.
- Existing marker ids and attributes from prior phases remain unchanged.
