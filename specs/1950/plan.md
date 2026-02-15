# Issue 1950 Plan

Status: Reviewed

## Approach

1. Add `RlPayloadBundle` struct in `tau-training-types` containing
   `EpisodeTrajectory`, `AdvantageBatch`, and `CheckpointRecord`.
2. Add `RlBundleError` enum for deterministic cross-payload mismatch reasons.
3. Implement `RlPayloadBundle::validate()` that:
   - calls each payload's existing `validate()`
   - checks `trajectory_id` alignment
   - checks trajectory step count == advantage count
   - checks checkpoint global_step progression against trajectory step count
4. Add unit/regression tests for pass and fail cases mapped to C-01..C-04.

## Affected Areas

- `crates/tau-training-types/src/lib.rs`
- `specs/1950/spec.md`
- `specs/1950/plan.md`
- `specs/1950/tasks.md`

## Risks And Mitigations

- Risk: over-constraining checkpoint progression semantics.
  - Mitigation: use minimal monotonic check (`global_step >= step_count`) only.
- Risk: error surface ambiguity.
  - Mitigation: explicit typed mismatch variants with field labels.

## ADR

No dependency/protocol changes; ADR not required.
