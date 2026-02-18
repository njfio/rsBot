# Spec #2471 - add startup prompt template loader/renderer in tau-onboarding

Status: Implemented

## Problem Statement
Startup prompt composition currently supports only hardcoded composition order. Operators cannot provide a workspace template to control section arrangement.

## Acceptance Criteria
### AC-1 Valid workspace template is applied
Given `.tau/prompts/system.md.j2` exists and references supported placeholders, when `compose_startup_system_prompt_with_report` runs, then output prompt matches rendered template content.

### AC-2 Missing template preserves existing composition
Given workspace has no startup template, when composition runs, then output remains equivalent to legacy composition logic.

### AC-3 Invalid template placeholders fail closed
Given template references unknown or malformed placeholders, when composition runs, then default composition is returned and startup does not fail.

## Scope
In scope:
- Template file path: `.tau/prompts/system.md.j2`.
- Supported placeholders: `base_system_prompt`, `skills_section`, `identity_sections`, `default_system_prompt`.
- Deterministic fallback to default composition.

Out of scope:
- Control-flow templating syntax.
- Runtime template reload watchers.

## Conformance Cases
- C-01 (AC-1, integration): `integration_spec_2471_c01_compose_startup_system_prompt_renders_workspace_template_placeholders`
- C-02 (AC-2, regression): `regression_spec_2471_c02_compose_startup_system_prompt_without_template_preserves_legacy_composition`
- C-03 (AC-3, regression): `regression_spec_2471_c03_compose_startup_system_prompt_invalid_template_placeholder_falls_back_to_default`

## Success Metrics
- C-01..C-03 pass in `tau-onboarding` tests.
- No startup composition panic on invalid template content.
