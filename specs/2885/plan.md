# Plan: Issue #2885 - session branch creation and lineage contracts

## Approach
1. Add additive row-level session branch form markers in sessions timeline rows within `tau-dashboard-ui`.
2. Add gateway branch form parser + handler that exports lineage up to selected entry into branch session path and redirects to target chat session.
3. Wire branch endpoint into gateway router and snapshot contracts.
4. Add RED UI/gateway conformance tests, implement minimum GREEN behavior, then run scoped regressions and validation gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: branch action may overwrite existing session data.
  - Mitigation: tests exercise unique target session keys and behavior is documented as deterministic by submitted key.
- Risk: selected entry id may be invalid.
  - Mitigation: fail closed by redirecting without mutation when selected entry is missing.
- Risk: brittle HTML assertions.
  - Mitigation: assert deterministic IDs/data attributes and key route markers only.

## Interface / Contract Notes
- New endpoint contract: `POST /ops/sessions/branch`.
- Existing `/ops/chat`, `/ops/sessions`, and `/ops/sessions/{session_key}` route contracts remain additive.
- No wire-format changes to existing JSON APIs.
