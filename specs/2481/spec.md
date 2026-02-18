# Spec #2481 - startup template minijinja migration with alias compatibility

Status: Implemented

## Problem Statement
Operators editing `.tau/prompts/system.md.j2` expect Jinja semantics and Spacebot-style variable names, but startup rendering currently depends on a custom placeholder parser.

## Acceptance Criteria
### AC-1 Startup templates render with minijinja
Given a valid startup template, when startup prompt composition runs, then rendering uses minijinja semantics and outputs deterministic content.

### AC-2 Alias variables are supported for startup context
Given templates using `identity`, `tools`, `memory_bulletin`, and `active_workers`, when composition runs, then rendering succeeds and aliases map to startup-safe values.

### AC-3 Fallback diagnostics remain fail-closed
Given template parse/render errors, when composition runs, then workspace render fails closed to builtin/default fallback with existing diagnostic reporting.

## Scope
In scope:
- Minijinja renderer integration for startup composition.
- Alias mapping for startup-safe variables.
- Conformance + regression coverage.

Out of scope:
- Runtime context population for live bulletin/worker lists.
- Hot reload integration.

## Conformance Cases
- C-01 (AC-1): workspace template with minijinja variables renders successfully.
- C-02 (AC-2): alias variable template renders with deterministic startup values.
- C-03 (AC-3): invalid minijinja expression triggers fallback diagnostics without regressing legacy behavior.

## Success Metrics
- C-01..C-03 pass in `tau-onboarding` suite.
