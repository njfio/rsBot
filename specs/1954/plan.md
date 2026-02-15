# Issue 1954 Plan

Status: Reviewed

## Approach

1. Extend adapter module with `TrajectoryWindowPolicy` and `TrajectoryPaddingMode`.
2. Add `SpansToTrajectories::with_window_policy(...)` constructor and policy
   validation (`window_size > 0` when provided).
3. Apply post-build transform to trajectory `steps`:
   - truncate to tail window when `steps.len() > window_size`
   - optionally pad to `window_size` when `steps.len() < window_size`
   - reindex `step_index` after transformation
   - ensure only final step has `done=true` (for padded outputs)
4. Add conformance tests C-01..C-04 mapped to AC-1..AC-4.

## Affected Areas

- `crates/tau-algorithm/src/adapters.rs`
- `specs/1954/spec.md`
- `specs/1954/plan.md`
- `specs/1954/tasks.md`

## Risks And Mitigations

- Risk: padding could inject malformed values.
  - Mitigation: deterministic synthetic step payload and schema validate final trajectory.
- Risk: behavior drift from existing adapter defaults.
  - Mitigation: keep default policy as no-op and enforce with compatibility test.

## ADR

No architecture/dependency/protocol changes; ADR not required.
