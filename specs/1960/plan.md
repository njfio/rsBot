# Issue 1960 Plan

Status: Reviewed

## Approach

1. Add `RolloutPersistenceAuditReport` and `AttemptPersistenceAudit` types in
   `tau-training-runner`.
2. Add async helper `audit_rollout_persistence(store, rollout_id)` that:
   - loads rollout by id
   - iterates expected attempt ids from `attempt_count`
   - queries attempt record + spans for each attempt
   - computes deterministic gap reasons
3. Treat terminal attempts (`Succeeded|Failed|Timeout|Unresponsive`) with zero
   spans as persistence gaps.
4. Add tests for C-01..C-04 including retry/requeue integration and a fault-injected
   store wrapper for missing-attempt gap detection.

## Affected Areas

- `crates/tau-training-runner/src/lib.rs`
- `specs/1960/spec.md`
- `specs/1960/plan.md`
- `specs/1960/tasks.md`

## Risks And Mitigations

- Risk: helper infers attempt ids from rollout `attempt_count`.
  - Mitigation: this matches store attempt id contract (`{rollout_id}:attempt-{n}`) and is deterministic.
- Risk: false positives for non-terminal in-flight attempts.
  - Mitigation: only enforce span presence for terminal attempt statuses.

## ADR

No dependency/protocol changes; ADR not required.
