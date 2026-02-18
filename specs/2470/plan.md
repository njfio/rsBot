# Plan #2470 - render workspace startup system prompt templates with deterministic placeholders

## Approach
- Extend `startup_prompt_composition` to build explicit composition sections.
- Resolve optional workspace template path from `.tau/prompts/system.md.j2`.
- Render placeholders via deterministic `{{name}}` substitution.
- Fall back to default composition on template errors.

## Affected Modules
- `crates/tau-onboarding/src/startup_prompt_composition.rs`

## Risks and Mitigations
- Risk: breaking legacy startup prompts.
Mitigation: explicit regression test comparing no-template behavior to legacy composition.

- Risk: template parse edge cases.
Mitigation: strict parser returning errors for malformed/missing placeholders and fallback to default.
