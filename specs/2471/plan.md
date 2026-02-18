# Plan #2471 - add startup prompt template loader/renderer in tau-onboarding

## Approach
1. Build deterministic default composition from base prompt + skills + identity sections.
2. Resolve optional workspace template path at `.tau/prompts/system.md.j2`.
3. Implement strict `{{placeholder}}` renderer for bounded placeholders.
4. On render/placeholder failure, emit warning and return default composition.
5. Add C-01..C-03 tests to `startup_prompt_composition` module.

## Interfaces
- `compose_startup_system_prompt_with_report` remains the primary API.
- New internal helpers for default composition, template loading, and rendering.

## Risks
- Behavior drift in no-template path.
Mitigation: explicit regression coverage for legacy composition invariants.
