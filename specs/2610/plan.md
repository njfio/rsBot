# Plan: Issue #2610 - Stale merged branch pruning safeguards

## Approach
1. Add script tests first using a temporary git repository fixture that models merged and protected remote branches.
2. Implement a dry-run-first pruning script that inventories candidates and supports explicit delete mode.
3. Emit reproducible JSON + Markdown audit artifacts under `tasks/reports/`.
4. Update stale-branch playbook with rollback and audit procedures tied to script outputs.
5. Run scoped verification commands and publish issue process evidence.

## Affected Modules
- `scripts/dev/stale-merged-branch-prune.sh` (new)
- `scripts/dev/test-stale-merged-branch-prune.sh` (new)
- `docs/guides/stale-branch-response-playbook.md` (update)
- `specs/2610/spec.md`
- `specs/2610/plan.md`
- `specs/2610/tasks.md`
- `specs/milestones/m104/index.md`

## Risks / Mitigations
- Risk: accidental deletion of active/protected branch names.
  - Mitigation: dry-run default, protected pattern allowlist, explicit `--execute` + `--confirm-delete` requirement.
- Risk: non-deterministic branch ordering causing flaky artifacts.
  - Mitigation: sort branch rows and normalize output formatting.
- Risk: fixture tests miss real-world edge names.
  - Mitigation: include protected prefixes and non-merged branch coverage in regression assertions.

## Interfaces / Contracts
- Script CLI contract:
  - Dry-run default.
  - Explicit destructive mode requires both `--execute` and `--confirm-delete`.
  - Emits JSON + Markdown reports with generated timestamp, base branch, candidate rows, and deletion actions.
- Audit contract:
  - Deletion rows must include branch tip SHA for rollback reconstruction.

## ADR
- Not required: no dependency additions or architecture changes.
