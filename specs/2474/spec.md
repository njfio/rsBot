# Spec #2474 - G17 prompt templates phase-2 orchestration

Status: Accepted

## Problem Statement
G17 phase 1 delivered workspace template rendering, but Tau still lacks explicit built-in template defaults and traceable template-source diagnostics.

## Acceptance Criteria
### AC-1 Phase-2 scope is explicitly bounded
Given milestone M81, when work executes, then scope is limited to startup prompt template fallback and diagnostics in `tau-onboarding`.

### AC-2 Child artifacts close with traceable conformance
Given #2475/#2476/#2477, when work is complete, then child specs and closure comments provide AC-to-test evidence.

## Scope
In scope:
- Phase-2 orchestration artifacts.
- Startup prompt template source fallback + diagnostics.

Out of scope:
- Runtime watchers and full hot-reload.
- New template dependencies.

## Conformance Cases
- C-01 (AC-1, governance): `specs/milestones/m81/index.md` and child specs define bounded phase-2 scope.
- C-02 (AC-2, governance): #2476/#2477 include AC-mapped conformance and RED/GREEN evidence.

## Success Metrics
- M81 closes with child issues closed and specs marked Implemented.
