# Plan: Issue #2738 - G18 embedded dashboard shell route

## Approach
1. Add RED tests for new shell markers and `/dashboard` endpoint contract.
2. Add embedded shell renderer module and deterministic HTML shell content.
3. Wire new `/dashboard` route in gateway router and status report payload.
4. Run regression suite for existing webchat/dashboard behavior.
5. Run scoped verification + live localhost route smoke and update checklist evidence.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/dashboard_shell_page.rs` (new)
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: new route or status field may drift existing status contract assertions.
  - Mitigation: update/add deterministic integration assertions.
- Risk: shell route introduces accidental auth coupling.
  - Mitigation: mirror `/webchat` public-shell behavior and verify with functional endpoint test.

## Interfaces / Contracts
- New endpoint: `GET /dashboard` (HTML shell).
- Existing endpoint extension: `GET /gateway/status` adds `gateway.dashboard_shell_endpoint`.
- No breaking changes to existing endpoints.

## ADR
- Not required: no dependency/protocol change.
