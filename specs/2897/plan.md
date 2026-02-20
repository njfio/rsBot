# Plan: Issue #2897 - session detail complete message coverage contracts

## Approach
1. Add RED gateway integration tests seeding sessions with mixed-role and empty-content entries, asserting complete detail timeline coverage contracts.
2. Add RED UI tests asserting timeline entry-count metadata consistency and deterministic row contracts for complete coverage.
3. Implement behavior hardening only if RED tests expose gaps in detail rendering.
4. Run required regression suites and verification gates (fmt/clippy/scoped tests/mutation/live validation).

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- (Implementation fallback if needed) `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`

## Risks and Mitigations
- Risk: existing runtime path already satisfies behavior; RED may require only stronger tests.
  - Mitigation: accept test-only change if behavior is already correct and verified.
- Risk: brittle string assertions in SSR HTML tests.
  - Mitigation: assert deterministic IDs/data attributes and content markers only.
- Risk: regression blast radius in session/chat suites.
  - Mitigation: rerun full required suite matrix before PR.

## Interface / Contract Notes
- No new endpoints.
- No protocol/schema changes.
- Contracts are additive test-hardening for existing detail behavior.
