# Spec #2470 - render workspace startup system prompt templates with deterministic placeholders

Status: Implemented

## Problem Statement
Operators cannot shape startup system prompt structure from workspace files. Prompt composition is fixed in Rust.

## Acceptance Criteria
### AC-1 Optional workspace template controls final startup prompt
Given a startup workspace template file and valid placeholders, when startup composition runs, then final system prompt follows template layout.

### AC-2 Legacy behavior is preserved when template is missing
Given no workspace template file, when startup composition runs, then existing prompt composition behavior is unchanged.

### AC-3 Invalid template usage is fail-closed
Given invalid placeholders or unreadable template content, when composition runs, then startup falls back to deterministic default composition.

## Scope
In scope:
- Optional template file in `.tau/prompts/system.md.j2`.
- Deterministic placeholder rendering for bounded variables.
- Fallback behavior tests.

Out of scope:
- Jinja control flow/filters.
- Template file watchers.

## Conformance Cases
- C-01 (AC-1, functional): valid template renders expected prompt arrangement.
- C-02 (AC-2, regression): missing template preserves pre-existing composition output.
- C-03 (AC-3, regression): missing placeholder falls back to default composition.

## Success Metrics
- C-01..C-03 pass in `tau-onboarding` tests.
