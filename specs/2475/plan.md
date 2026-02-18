# Plan #2475 - startup prompt template source fallback and diagnostics

## Approach
- Add built-in startup template asset for deterministic fallback.
- Extend startup prompt composition output with template-source diagnostics.
- Resolve template source in strict order: workspace -> builtin -> default fallback.
- Keep no-template behavior compatible with existing startup composition.

## Affected Modules
- `crates/tau-onboarding/src/startup_prompt_composition.rs`

## Risks and Mitigations
- Risk: startup prompt drift in compatibility path.
  Mitigation: retain legacy default composition and assert deterministic regression outputs.
