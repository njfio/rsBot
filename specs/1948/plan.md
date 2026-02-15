# Issue 1948 Plan

Status: Reviewed

## Approach

1. Extend `PpoConfig` with `epochs` and validate `epochs > 0`.
2. Update `compute_ppo_update` to iterate minibatches across epochs while
   preserving deterministic step indexing.
3. Add/extend guardrails for non-finite range checks in update summaries.
4. Add tests-first coverage for epoch accounting, invalid config, and guardrail
   failure paths.

## Affected Areas

- `crates/tau-algorithm/src/ppo.rs`
- `specs/1948/spec.md`
- `specs/1948/plan.md`
- `specs/1948/tasks.md`

## Risks And Mitigations

- Risk: epoch loop changes existing deterministic step indexes.
  - Mitigation: codify explicit expected index behavior in regression tests.
- Risk: over-strict guards may reject valid edge-case updates.
  - Mitigation: keep guard thresholds tied to finite checks and explicit config
    constraints rather than heuristic cutoffs.

## ADR

No dependency/protocol changes; ADR not required.
