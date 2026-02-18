# Spec #2531 - Subtask: RED/GREEN + live validation evidence for G14 adapters

Status: Reviewed

## Problem Statement
Adapter changes must ship with explicit evidence that behavior was red-first, then green, and validated end-to-end.

## Acceptance Criteria
### AC-1 red/green evidence
Given conformance tests, when work is submitted, then RED and GREEN command evidence is captured in the PR.

### AC-2 validation package completeness
Given implementation is complete, when verification runs, then tier matrix, mutation results, and live validation results are included.

## Conformance Cases
- C-01 (AC-1): RED failing test commands captured for new send-file adapter tests.
- C-02 (AC-1): GREEN passing test commands captured for same tests.
- C-03 (AC-2): mutation and live validation results attached in PR body.

## Success Metrics
- PR contains complete evidence package with no missing required sections.
