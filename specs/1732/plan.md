# Issue 1732 Plan

Status: Reviewed

## Approach

1. Audit GitHub milestone taxonomy for stale RL naming outside approved
   future true-RL milestone `#24`.
2. Patch stale milestone metadata (`#6`) to prompt-optimization naming.
3. Refresh roadmap docs with explicit naming-alignment taxonomy references.
4. Commit before/after audit artifacts under `tasks/reports/`.
5. Run docs link checks for regression safety.

## Affected Areas

- GitHub milestone metadata (`#6`)
- `docs/guides/roadmap-execution-index.md`
- `docs/README.md`
- `tasks/reports/m22-taxonomy-rename-audit.md`
- `tasks/reports/m22-taxonomy-rename-audit.json`
- `specs/1732/*`

## Risks And Mitigations

- Risk: renaming historical milestone confuses existing references
  - Mitigation: preserve milestone number, update only title/description, and
    include explicit before/after in report artifact.
- Risk: docs drift after taxonomy updates
  - Mitigation: run docs link checks and include updated cross-links.

## ADR

No architecture/dependency/protocol changes. ADR not required.
