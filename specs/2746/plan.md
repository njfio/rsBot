# Plan: Issue #2746 - G18 dashboard architecture/stack decision ADR closure

## Approach
1. Add ADR-006 with explicit architecture/stacк decision and migration consequences.
2. Update G18 checklist rows for architecture decision and tech stack selection.
3. Run lightweight verification gates (`fmt`, `clippy`) to ensure no incidental code regressions.

## Affected Modules
- `docs/architecture/adr-006-dashboard-ui-stack.md` (new)
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: decision wording could conflict with previous dashboard docs.
  - Mitigation: align ADR wording with existing delivered gateway `/dashboard` shell and planned incremental migration.

## Interfaces / Contracts
- Documentation/governance only; no runtime API changes.

## ADR
- This task produces ADR-006.
