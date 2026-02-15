# Issue 1726 Plan

Status: Reviewed

## Approach

1. Extend `tau-trainer::checkpoint_store` with deterministic operator diagnostic
   rendering for resume outcomes.
2. Add tests-first coverage for:
   - combined corruption failure diagnostics
   - primary-preferred rollback safety behavior
   - operator diagnostics output contract
3. Keep implementation scoped to checkpoint restore behavior and diagnostics only.
4. Run scoped verification and map AC/C-xx evidence in PR.

## Affected Areas

- `crates/tau-trainer/src/checkpoint_store.rs`
- `specs/1726/spec.md`
- `specs/1726/plan.md`
- `specs/1726/tasks.md`

## Risks And Mitigations

- Risk: diagnostics output instability causing brittle operator tooling.
  - Mitigation: deterministic formatting with explicit tests.
- Risk: rollback behavior regressions from future refactors.
  - Mitigation: keep integration tests covering primary-vs-fallback selection.

## ADR

No architecture dependency or protocol change; ADR not required.
