# Plan: Issue #2606 - Validate tau-gaps roadmap items and execute open P0/P1 remediations

## Approach
1. Update roadmap evidence table and follow-up index in `tasks/tau-gaps-issues-improvements.md` for newly completed tasks.
2. Reconcile `specs/milestones/m104/index.md` with actual closed task set and milestone completion state.
3. Run docs validation checks used by CI for changed files.
4. Publish story closeout summary on issue #2606 and close the issue.

## Affected Modules
- `tasks/tau-gaps-issues-improvements.md`
- `specs/milestones/m104/index.md`
- `specs/2606/spec.md`
- `specs/2606/plan.md`
- `specs/2606/tasks.md`

## Risks / Mitigations
- Risk: stale manual edits drift from actual issue state.
  - Mitigation: verify with `gh issue list/view` before writing closeout updates.
- Risk: docs links/format drift.
  - Mitigation: run docs quality checks before PR.

## Interfaces / Contracts
- Story issue #2606 closeout contract comment format from `AGENTS.md`.
- Milestone index (`specs/milestones/m104/index.md`) as source-of-truth for M104 status.

## ADR
- Not required (documentation/state reconciliation only).
