# Plan: Issue #3276 - move openresponses execution handler to dedicated module

## Approach
1. RED: tighten root guard threshold and assert `execute_openresponses_request` is not declared in root.
2. Add `openresponses_execution_handler.rs` and move `execute_openresponses_request` implementation.
3. Import moved function into root so openresponses handler/stream flow remains unchanged.
4. Verify with openresponses execution tests, guard script, fmt, clippy.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/openresponses_execution_handler.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m246/index.md`
- `specs/3276/spec.md`
- `specs/3276/plan.md`
- `specs/3276/tasks.md`

## Risks & Mitigations
- Risk: stream-delta or usage persistence drift.
  - Mitigation: targeted functional/integration tests in AC-1.
- Risk: visibility/import wiring regressions.
  - Mitigation: explicit root import and compile verification.

## Interfaces / Contracts
- Public endpoint paths unchanged.
- Response schema and stream event contracts unchanged.

## ADR
No ADR required (internal handler extraction only).
