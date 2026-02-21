# Plan: Issue #3268 - move auth-session handler to dedicated module

## Approach
1. RED: tighten root guard threshold and assert `handle_gateway_auth_session` is not declared in root.
2. Add `auth_session_handler.rs` and move `handle_gateway_auth_session` implementation.
3. Import moved handler into root so router wiring remains unchanged.
4. Verify with auth-session tests, guard script, fmt, clippy.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/auth_session_handler.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m244/index.md`
- `specs/3268/spec.md`
- `specs/3268/plan.md`
- `specs/3268/tasks.md`

## Risks & Mitigations
- Risk: endpoint wiring visibility/import regression.
  - Mitigation: explicit root import and auth-session functional/regression tests.
- Risk: behavior drift in malformed-json or mode checks.
  - Mitigation: existing regression tests and conformance mapping.

## Interfaces / Contracts
- Public endpoint path unchanged (`/gateway/auth/session`).
- Response/error contract unchanged.

## ADR
No ADR required (internal handler extraction only).
