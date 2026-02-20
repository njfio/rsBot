# Plan: Issue #2905 - ops memory search relevant result contracts

## Approach
1. Add RED UI tests for memory search panel/form markers, query preservation, and empty-state/result counters.
2. Add RED gateway integration tests that seed persisted memory entries and assert relevant `/ops/memory` result rows.
3. Implement minimal memory snapshot plumbing in ops shell and memory panel rendering contracts in `tau-dashboard-ui`.
4. Run regression + verify gates (fmt/clippy/spec slices/mutation/live validation).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: adding new snapshot fields can break many existing shell tests.
  - Mitigation: add defaults and additive rendering markers only; preserve existing marker IDs.
- Risk: memory search relevance can vary by backend.
  - Mitigation: seed deterministic entries and assert target entry IDs/summary fragments.
- Risk: route-level regressions in chat/session panels.
  - Mitigation: rerun established chat/session/detail regression slices.

## Interface / Contract Notes
- No new HTTP endpoints.
- No protocol/schema changes.
- Additive SSR marker contracts on existing `/ops/memory` route.
