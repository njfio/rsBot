# Spec #2469 - G17 startup prompt template phase-1 orchestration

Status: Reviewed

## Problem Statement
G17 requires startup prompt templates, but this phase must be delivered as a bounded, low-risk slice without dependency expansion.

## Acceptance Criteria
### AC-1 Scope is explicitly bounded for phase-1
Given milestone M80, when work is executed, then scope is restricted to workspace startup prompt template rendering in `tau-onboarding`.

### AC-2 Child artifacts are complete and traceable
Given issue hierarchy #2470/#2471/#2472, when implementation is complete, then each child has AC-linked conformance evidence and closure notes.

## Scope
In scope:
- Phase-1 orchestration artifacts and links.
- Startup prompt template rendering slice.

Out of scope:
- Runtime watcher/hot-reload.
- New prompt templating dependencies.

## Conformance Cases
- C-01 (AC-1, governance): `specs/milestones/m80/index.md` and child specs define bounded scope.
- C-02 (AC-2, governance): #2471 contains AC->test mapping and #2472 contains RED/GREEN evidence.

## Success Metrics
- M80 closes with all child issues closed and specs marked Implemented.
