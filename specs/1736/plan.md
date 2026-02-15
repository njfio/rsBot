# Issue 1736 Plan

Status: Reviewed

## Approach

1. Extend `TrainingStore` with timeout-reassignment operation keyed by heartbeat
   timeout duration.
2. Implement reassignment in both in-memory and sqlite store backends:
   - timeout stale running attempts
   - transition rollout to `requeuing`
   - clear worker active assignment
3. Update `TrainingRunner` loop with periodic reassignment checks and
   stale-attempt guard before final status updates.
4. Add integration/regression chaos tests across store and runner crates.

## Affected Areas

- `crates/tau-training-store/src/lib.rs`
- `crates/tau-training-store/src/sqlite.rs`
- `crates/tau-training-runner/src/lib.rs`
- `crates/tau-trainer/src/lib.rs`
- `specs/1736/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: duplicate completion writes from stale worker finishing late.
  - Mitigation: runner checks attempt status before finalization and skips stale
    completions.
- Risk: inconsistent backend behavior.
  - Mitigation: mirror timeout-reassignment semantics in in-memory and sqlite
    plus backend-specific tests.

## ADR

No architecture/dependency/protocol change. ADR not required.
