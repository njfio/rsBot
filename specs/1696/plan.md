# Issue 1696 Plan

Status: Reviewed

## Approach

1. Add targeted GAE tests in `tau-algorithm/src/gae.rs` for:
   - truncated non-terminal trajectory with bootstrap
   - terminal bootstrap masking correctness
   - sparse reward horizon stability
2. Validate each case for deterministic behavior and finite outputs.
3. Keep changes test-only unless edge-case behavior reveals a bug.

## Affected Areas

- `crates/tau-algorithm/src/gae.rs`
- `specs/1696/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: edge-case expectations may be ambiguous.
  - Mitigation: encode explicit formula assertions in tests.
- Risk: flaky assertions due floating-point drift.
  - Mitigation: use tolerance-based comparisons where needed.

## ADR

No architecture/protocol/dependency boundary change. ADR not required.
