# Plan: Issue #3244 - move gateway bootstrap/router wiring into module

## Approach
1. RED: tighten root size guard to `860` and add ownership assertions that bootstrap/router function definitions are not declared in root.
2. Add `server_bootstrap.rs` for bootstrap/runtime startup and router assembly functions.
3. Wire root module with `mod server_bootstrap;` and `pub use server_bootstrap::run_gateway_openresponses_server;`.
4. Verify with guard, focused integration tests, fmt, clippy.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/server_bootstrap.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m238/index.md`
- `specs/3244/spec.md`
- `specs/3244/plan.md`
- `specs/3244/tasks.md`

## Risks & Mitigations
- Risk: API path regression for public startup function.
  - Mitigation: root `pub use` re-export and conformance integration tests.
- Risk: route wiring import visibility breaks in extracted module.
  - Mitigation: use `super::*` within bootstrap module and compile/test gates.

## Interfaces / Contracts
- `run_gateway_openresponses_server` remains reachable from existing module path.
- Router endpoint wiring/handler mapping remains behaviorally unchanged.

## ADR
No ADR required (internal module extraction only; no protocol/contract changes).
