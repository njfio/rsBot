# M80 - Spacebot G17 Prompt Templates (Phase 1)

## Context
`tasks/spacebot-comparison.md` identifies `G17` as a gap: Tau startup prompts are composed in code and cannot be operator-templated from workspace files.

This milestone delivers a bounded first slice: optional workspace startup prompt template rendering in `tau-onboarding` with deterministic placeholders and fail-closed fallback behavior.

## Scope
- Optional template file resolution in workspace startup path.
- Deterministic placeholder rendering for startup prompt sections.
- Conformance + regression coverage for render, fallback, and compatibility.

## Out of Scope
- New dependencies (`minijinja`) in this slice.
- File watch / hot-reload for template changes.
- Process-type-specific template selection.

## Issue Hierarchy
- Epic: #2469
- Story: #2470
- Task: #2471
- Subtask: #2472

## Verification Targets
- `cargo test -p tau-onboarding -- startup_prompt_composition`
- `cargo clippy -p tau-onboarding -- -D warnings`
- `cargo mutants -p tau-onboarding --in-diff`
