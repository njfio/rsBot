# Spec #2480 - G17 phase-3 orchestration for minijinja startup templates

Status: Implemented

## Problem Statement
Phase-2 startup template behavior is in place, but rendering still uses a custom placeholder parser and does not expose Spacebot-style alias names expected by operators.

## Acceptance Criteria
### AC-1 Phase-3 scope is bounded and traceable
Given milestone M82, when implementation executes, then changes are limited to startup template engine migration and startup-safe alias support.

### AC-2 Child issues provide conformance evidence
Given #2481/#2482/#2483, when work completes, then AC-to-test mapping and RED/GREEN evidence are present in specs and PR(s).

## Scope
In scope:
- Milestone and issue-level phase-3 orchestration artifacts.
- Startup template engine and alias compatibility slice.

Out of scope:
- Runtime hot-reload and process prompt unification across crates.

## Conformance Cases
- C-01 (AC-1, governance): milestone index and child specs explicitly bound phase-3 scope.
- C-02 (AC-2, governance): child task/subtask include conformance + RED/GREEN evidence.

## Success Metrics
- M82 closes with all child specs marked Implemented and issue closure comments complete.
