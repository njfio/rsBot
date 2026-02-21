# Plan: Issue #2963 - Gateway API reference documentation

## Approach
1. Inspect gateway route constants and router bindings from `crates/tau-gateway/src/gateway_openresponses.rs`.
2. Produce a grouped API reference with method/path/auth/purpose columns.
3. Include policy-gate notes for write/reset endpoints.
4. Link the reference from `docs/README.md`.
5. Validate route coverage using extraction checks against the route table and run docs quality scripts.

## Affected Paths
- `docs/guides/gateway-api-reference.md` (new)
- `docs/README.md` (index link)

## Risks and Mitigations
- Risk: route drift and omissions.
  - Mitigation: derive inventory from route constants + route declarations; include coverage check evidence.
- Risk: auth semantics ambiguity.
  - Mitigation: explicitly map endpoint groups to token/password-session/localhost-dev behavior and policy gates.

## Interfaces / Contracts
- `build_gateway_openresponses_router` route definitions in `gateway_openresponses.rs`
- auth model (`token`, `password-session`, `localhost-dev`) already defined in gateway docs/runtime
- policy gates: `allow_session_write`, `allow_memory_write`

## ADR
Not required (documentation-only).
