# Spec #2482 - migrate startup template rendering to minijinja with alias context

Status: Implemented

## Problem Statement
`render_prompt_template` currently parses `{{...}}` manually, limiting template semantics and requiring non-Spacebot variable names.

## Acceptance Criteria
### AC-1 Workspace minijinja template renders with startup context
Given workspace `.tau/prompts/system.md.j2` containing supported variables, when `compose_startup_system_prompt_with_report` runs, then output is rendered via minijinja and `template_report.source` remains `workspace`.

### AC-2 Spacebot-style alias variables are accepted
Given template variables `identity`, `tools`, `memory_bulletin`, and `active_workers`, when startup composition runs, then rendering succeeds and values are deterministic (`identity` and `tools` mapped from startup composition data; `memory_bulletin` and `active_workers` default to empty startup-safe values).

### AC-3 Invalid minijinja template fails closed with fallback diagnostics
Given template parse/render error, when startup composition runs, then output excludes invalid workspace content and source diagnostics indicate builtin/default fallback.

## Scope
In scope:
- Add `minijinja` workspace dependency and adopt it in startup renderer.
- Maintain legacy variable names (`base_system_prompt`, `skills_section`, `identity_sections`, `default_system_prompt`).
- Add alias mappings for startup-safe Spacebot names.
- Regression preservation for existing spec_2471/spec_2476 tests.

Out of scope:
- Runtime bulletin population beyond deterministic startup default.
- Agent-core prompt templating migration.

## Conformance Cases
- C-01 (AC-1, integration): `integration_spec_2482_c01_workspace_template_renders_with_minijinja`
- C-02 (AC-2, integration): `integration_spec_2482_c02_alias_variables_render_with_startup_safe_values`
- C-03 (AC-3, regression): `regression_spec_2482_c03_invalid_minijinja_template_falls_back_to_builtin_source`
- C-04 (AC-2/AC-3, regression): `regression_spec_2482_c04_builtin_template_supports_aliases_and_preserves_default_fallback`

## Success Metrics
- C-01..C-04 pass in `tau-onboarding`.
- Existing `spec_2471` and `spec_2476` tests remain green.
