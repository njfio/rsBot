# Plan: Issue #3280 - move gateway root utility helpers to dedicated module

## Approach
1. RED: tighten root guard threshold and assert helper functions are not declared in root.
2. Add `root_utilities.rs` and move helper implementations.
3. Import moved helpers into root to preserve call sites.
4. Verify with helper-focused regression/integration tests, guard script, fmt, clippy.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/root_utilities.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m247/index.md`
- `specs/3280/spec.md`
- `specs/3280/plan.md`
- `specs/3280/tasks.md`

## Risks & Mitigations
- Risk: helper visibility breakage for call sites/tests.
  - Mitigation: explicit root imports and targeted regression/integration tests.
- Risk: minor behavior drift in bind parsing.
  - Mitigation: bind regression test coverage.

## Interfaces / Contracts
- Public endpoint/API contracts unchanged.
- Utility helper contracts unchanged.

## ADR
No ADR required (internal utility extraction only).
