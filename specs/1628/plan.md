# Issue 1628 Plan

Status: Reviewed

## Approach

1. Tests-first: update `scripts/dev/test-training-crate-boundary-plan.sh` to
   require set-c completion semantics for `#1628`.
2. Update generator defaults in `scripts/dev/training-crate-boundary-plan.sh`
   so set-c is completed with explicit retention scope.
3. Regenerate artifacts:
   - `tasks/reports/training-crate-boundary-plan.json`
   - `tasks/reports/training-crate-boundary-plan.md`
4. Update guide:
   - `docs/guides/training-crate-boundary-plan.md`
5. Run scoped verification:
   - boundary plan test harness
   - targeted training crate test commands

## Affected Areas

- `scripts/dev/training-crate-boundary-plan.sh`
- `scripts/dev/test-training-crate-boundary-plan.sh`
- `tasks/reports/training-crate-boundary-plan.json`
- `tasks/reports/training-crate-boundary-plan.md`
- `docs/guides/training-crate-boundary-plan.md`
- `specs/1628/spec.md`
- `specs/1628/plan.md`
- `specs/1628/tasks.md`

## Risks And Mitigations

- Risk: status drift between guide and generated artifacts.
  - Mitigation: regenerate artifacts from script and update guide in same PR.
- Risk: retained boundaries hide compile regressions.
  - Mitigation: run targeted training crate tests as integration verification.

## ADR

No dependency/protocol changes; ADR not required.
