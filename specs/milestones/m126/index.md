# M126 - Spacebot G10 Serenity Runtime Foundation

## Context
`tasks/spacebot-comparison.md` still has two unchecked G10 foundation items:
- Add `serenity` as workspace dependency
- Create `tau-discord-runtime` crate or add equivalent modularization to `tau-multi-channel`

## Linked Work
- Epic: #2768
- Story: #2769
- Task: #2770
- Source parity checklist: `tasks/spacebot-comparison.md` (G10)

## Scope
- Introduce `serenity` dependency with explicit contract/ADR traceability.
- Create and wire baseline Discord runtime crate/module foundation.
- Preserve existing multi-channel behavior and test coverage.

## Exit Criteria
- G10 remaining foundation rows are complete with issue evidence.
- Scoped quality gates and integration tests pass.
- Spec artifacts for #2770 move to Implemented.
