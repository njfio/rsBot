# M81 - Spacebot G17 Prompt Templates (Phase 2)

Milestone: [GitHub milestone #81](https://github.com/njfio/Tau/milestone/81)

## Context
`tasks/spacebot-comparison.md` gap `G17` requires prompt templates that can be operator-edited with built-in defaults.

Phase 1 added optional workspace startup template rendering. Phase 2 adds deterministic built-in default template fallback plus template-source diagnostics, while preserving startup prompt compatibility.

## Scope
- Add built-in startup prompt template default for composition fallback.
- Resolve template source deterministically (`workspace` -> `builtin` -> `default`).
- Add template-source diagnostics to startup composition output.
- Add conformance/regression coverage for source selection and fallback behavior.

## Out of Scope
- Adding new template-engine dependencies.
- Runtime file watching / hot reload.
- Process-type-specific prompt templates.

## Linked Hierarchy
- Epic: #2474
- Story: #2475
- Task: #2476
- Subtask: #2477

## Verification Targets
- `cargo test -p tau-onboarding -- spec_2476`
- `cargo clippy -p tau-onboarding -- -D warnings`
- `cargo mutants -p tau-onboarding --in-diff`
