# Spec #2476 - implement startup template builtin fallback and source diagnostics

Status: Implemented

## Problem Statement
Startup prompt composition currently reports no template source metadata and only supports workspace template selection with silent default fallback.

## Acceptance Criteria
### AC-1 Workspace template diagnostics
Given `.tau/prompts/system.md.j2` exists and renders successfully, when `compose_startup_system_prompt_with_report` runs, then `template_report.source` is `workspace` and reason code indicates workspace rendering success.

### AC-2 Builtin fallback diagnostics
Given workspace template is missing/unreadable/invalid-type/empty, when composition runs, then `template_report.source` is `builtin` and reason code indicates builtin fallback path.

### AC-3 Invalid workspace template fail-closed with compatibility
Given workspace template contains unsupported placeholders, when composition runs, then output excludes invalid template content and `template_report` indicates builtin fallback; if builtin rendering fails, output falls back to default composition.

## Scope
In scope:
- Built-in template asset inclusion.
- `StartupPromptTemplateReport` diagnostics in startup composition result.
- Deterministic source resolution and fallback behavior.

Out of scope:
- Runtime template watcher.
- Dependency additions for alternate template engines.

## Conformance Cases
- C-01 (AC-1, integration): `integration_spec_2476_c01_compose_startup_system_prompt_reports_workspace_template_source`
- C-02 (AC-2, integration): `integration_spec_2476_c02_compose_startup_system_prompt_without_workspace_template_uses_builtin_source`
- C-03 (AC-3, regression): `regression_spec_2476_c03_invalid_workspace_template_falls_back_to_builtin_source`

## Success Metrics
- C-01..C-03 pass in `tau-onboarding`.
- Existing `spec_2471` regression tests stay green.
