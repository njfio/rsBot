# Issue 1719 Plan

Status: Reviewed

## Approach

1. Add issue-level spec artifacts under `specs/1719/`.
2. Patch high-visibility docs:
   - `README.md`
   - `docs/README.md`
   - `docs/guides/training-ops.md`
3. Introduce explicit dual-track wording and cross-links:
   - current implemented prompt optimization
   - future true RL roadmap via Epic `#1657` and Milestone `#24`
4. Run docs link checks to validate no regressions.

## Affected Areas

- `README.md`
- `docs/README.md`
- `docs/guides/training-ops.md`
- `specs/1719/spec.md`
- `specs/1719/plan.md`
- `specs/1719/tasks.md`

## Risks And Mitigations

- Risk: wording ambiguity around roadmap status
  - Mitigation: use explicit "future/planned" phrasing and direct links.
- Risk: documentation link regressions
  - Mitigation: run docs link checks before PR.

## ADR

No architecture or dependency change. ADR not required.
