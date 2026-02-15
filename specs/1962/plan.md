# Issue 1962 Plan

Status: Reviewed

## Approach

1. Add `CollectorPersistenceProof` and `CollectorStatusTotals` in
   `tau-training-runner`.
2. Add `build_collector_persistence_proof(store, rollout_ids)`:
   - deterministic sorted/deduped rollout ids
   - per-id call to `audit_rollout_persistence`
   - aggregate status/attempt/span/gap totals
3. Add `to_artifact_json()` projection for machine-readable proof output.
4. Add tests C-01..C-04 covering clean, retry/requeue, gap propagation, and
   JSON artifact shape.

## Affected Areas

- `crates/tau-training-runner/src/lib.rs`
- `specs/1962/spec.md`
- `specs/1962/plan.md`
- `specs/1962/tasks.md`

## Risks And Mitigations

- Risk: non-deterministic input ordering.
  - Mitigation: sort/dedup rollout ids before aggregation.
- Risk: artifact projection drift.
  - Mitigation: lock required fields in unit test.

## ADR

No dependency/protocol changes; ADR not required.
