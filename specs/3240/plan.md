# Plan: Issue #3240 - move gateway server config/state types into module

## Approach
1. RED: tighten root size guard to `1040` and add ownership checks for server config/state type definitions in root.
2. Add `server_state.rs` with config/state type definitions and state helper methods.
3. Re-export config type from root and wire state type import usage.
4. Verify with guard, focused integration tests, and quality gates.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/server_state.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m237/index.md`
- `specs/3240/spec.md`
- `specs/3240/plan.md`
- `specs/3240/tasks.md`

## Risks & Mitigations
- Risk: field visibility regressions across sibling modules.
  - Mitigation: use `pub(super)` on state struct fields required by sibling modules.
- Risk: accidental API path breakage for public config type.
  - Mitigation: root `pub use` re-export and integration tests.

## Interfaces / Contracts
- `GatewayOpenResponsesServerConfig` remains available from root module path.
- Route and endpoint behavior unchanged.

## ADR
No ADR required (internal type extraction and re-export only).
