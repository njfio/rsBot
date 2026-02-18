# Spec #2475 - startup prompt template source fallback and diagnostics

Status: Accepted

## Problem Statement
Operators need deterministic startup prompt template behavior even when workspace template files are absent or invalid, and startup needs diagnostics that indicate which template source was used.

## Acceptance Criteria
### AC-1 Workspace template source remains primary
Given a valid workspace startup template, when startup composition runs, then rendered prompt uses workspace template and reports workspace as source.

### AC-2 Built-in template default is used when workspace template is unavailable
Given missing/unreadable/invalid-type/empty workspace template, when startup composition runs, then built-in template default is used and source diagnostics reflect builtin fallback.

### AC-3 Compatibility is preserved
Given phase-1 and legacy startup scenarios, when startup composition runs, then composed prompt behavior remains deterministic and does not regress.

## Scope
In scope:
- Built-in startup template default.
- Template source diagnostics on prompt composition output.
- Conformance + regression coverage.

Out of scope:
- Hot-reload watcher integration.
- Advanced Jinja control-flow features.

## Conformance Cases
- C-01 (AC-1, integration): valid workspace template yields workspace source diagnostics.
- C-02 (AC-2, integration): missing workspace template yields builtin source diagnostics.
- C-03 (AC-2/AC-3, regression): invalid workspace template falls back to builtin source while preserving deterministic output.

## Success Metrics
- C-01..C-03 pass in `tau-onboarding` tests.
