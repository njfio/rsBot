# Plan: Issue #3248 - move gateway ops shell handlers into module

## Approach
1. RED: tighten root size guard to `680` and add ownership checks for ops shell macro/detail handler definitions in root.
2. Add `ops_shell_handlers.rs` and move macro + ops shell handler glue functions.
3. Wire root with module import and bring moved handlers into scope for router wiring.
4. Verify with guard, focused integration tests, fmt, clippy.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_handlers.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m239/index.md`
- `specs/3248/spec.md`
- `specs/3248/plan.md`
- `specs/3248/tasks.md`

## Risks & Mitigations
- Risk: router references fail if moved handlers are not imported in root scope.
  - Mitigation: explicit `use ops_shell_handlers::{...};` plus integration tests.
- Risk: macro visibility/expansion regressions.
  - Mitigation: keep macro local to new module and expose concrete handlers only.

## Interfaces / Contracts
- Ops shell endpoints remain unchanged.
- Status/auth/runtime behavior unchanged.

## ADR
No ADR required (internal extraction only).
