# Plan: Issue #3260 - move websocket and stream handlers to dedicated module

## Approach
1. RED: tighten root guard and assert moved ws/stream handler function definitions are not declared in root.
2. Add `ws_stream_handlers.rs` and move ws/stream helper handlers.
3. Import moved helpers into root so route wiring remains unchanged.
4. Verify with ws/stream functional tests, guard script, fmt, clippy.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/ws_stream_handlers.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m242/index.md`
- `specs/3260/spec.md`
- `specs/3260/plan.md`
- `specs/3260/tasks.md`

## Risks & Mitigations
- Risk: moved function visibility/import regressions.
  - Mitigation: explicit imports and targeted ws/stream functional tests.
- Risk: subtle event-stream behavior drift.
  - Mitigation: existing stream contract tests in conformance set.

## Interfaces / Contracts
- Public endpoint paths unchanged.
- WS/session and stream payload/headers unchanged.

## ADR
No ADR required (internal handler extraction only).
