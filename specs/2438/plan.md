# Plan #2438

Status: Reviewed
Spec: specs/2438/spec.md

## Approach

1. Capture RED by running roadmap freshness check (currently failing in CI).
2. Run roadmap sync generator and stage only resulting docs diffs.
3. Implement `preflight-fast.sh` wrapper with strict shell options.
4. Add `test-preflight-fast.sh` using temp fixture stubs for deterministic
   pass/fail and arg passthrough checks.
5. Run green validation suite and push to PR #2435.

## Affected Modules (planned)

- `tasks/todo.md`
- `tasks/tau-vs-ironclaw-gap-list.md`
- `scripts/dev/preflight-fast.sh`
- `scripts/dev/test-preflight-fast.sh`
- `specs/milestones/m74/index.md`
- `specs/2438/spec.md`
- `specs/2438/plan.md`
- `specs/2438/tasks.md`

## Risks and Mitigations

- Risk: wrapper accidentally weakens safety checks.
  - Mitigation: enforce roadmap check first and fail fast on non-zero status.
- Risk: flaky tests due to environment coupling.
  - Mitigation: use local temp dirs and stub scripts for deterministic behavior.
