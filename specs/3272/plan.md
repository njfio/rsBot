# Plan: Issue #3272 - move openresponses entry handler to dedicated module

## Approach
1. RED: tighten root guard threshold and assert `handle_openresponses` is not declared in root.
2. Add `openresponses_entry_handler.rs` and move `handle_openresponses` implementation.
3. Import moved handler into root so router wiring remains unchanged.
4. Verify with openresponses entry tests, guard script, fmt, clippy.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/openresponses_entry_handler.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m245/index.md`
- `specs/3272/spec.md`
- `specs/3272/plan.md`
- `specs/3272/tasks.md`

## Risks & Mitigations
- Risk: stream/non-stream branch behavior drift.
  - Mitigation: existing functional tests for both branches.
- Risk: auth/body-limit parse behavior drift.
  - Mitigation: oversized-input regression + existing auth path coverage.

## Interfaces / Contracts
- Public endpoint path unchanged (`/v1/responses`).
- Stream and non-stream response schemas unchanged.

## ADR
No ADR required (internal handler extraction only).
