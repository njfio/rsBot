# Issue 1733 Plan

Status: Reviewed

## Approach

1. Add a planning document at `docs/planning/true-rl-roadmap-skeleton.md`.
2. Define staged phases with:
   - objective
   - in-scope milestones/issues
   - entry criteria
   - exit evidence
3. Link the planning doc from training/naming docs so users can navigate from
   current prompt-optimization docs to future true-RL roadmap details.
4. Run docs link checks for regression safety.

## Affected Areas

- `docs/planning/true-rl-roadmap-skeleton.md`
- `README.md`
- `docs/README.md`
- `docs/guides/training-ops.md`
- `docs/guides/roadmap-execution-index.md`
- `specs/1733/*`

## Risks And Mitigations

- Risk: roadmap doc drifts from GitHub issue state
  - Mitigation: use stable issue links grouped by current milestone-24 stories.
- Risk: boundary wording becomes ambiguous
  - Mitigation: keep explicit "current prompt optimization" vs "future true RL"
    language in cross-linked docs.

## ADR

No architecture/dependency/protocol changes. ADR not required.
