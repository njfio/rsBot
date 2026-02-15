# Issue 1952 Plan

Status: Reviewed

## Approach

1. Add `CheckpointLineageError` enum in `tau-training-types` for deterministic
   lineage failure modes.
2. Add `resolve_checkpoint_lineage_path(records, leaf_checkpoint_id)` helper:
   - validate each checkpoint record
   - build deterministic id->record map with duplicate detection
   - walk parent chain via `metadata["parent_checkpoint_id"]`
   - detect missing parents and cycles
   - return canonical root->leaf id vector
3. Add tests for success, duplicate ids, missing parent, cycle, and unknown
   leaf paths.

## Affected Areas

- `crates/tau-training-types/src/lib.rs`
- `specs/1952/spec.md`
- `specs/1952/plan.md`
- `specs/1952/tasks.md`

## Risks And Mitigations

- Risk: ambiguous parent field semantics.
  - Mitigation: use explicit metadata key `parent_checkpoint_id` and ignore
    non-string values.
- Risk: nondeterministic cycle/duplicate reporting.
  - Mitigation: deterministic iteration and typed errors with checkpoint ids.

## ADR

No dependency/protocol changes; ADR not required.
