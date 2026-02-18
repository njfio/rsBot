# Spec #2537 - Subtask: RED/GREEN + validation evidence for G15 profile routing

Status: Implemented

## Problem Statement
Task #2536 requires explicit RED/GREEN and full verification evidence to satisfy AGENTS merge gates.

## Acceptance Criteria
### AC-1 red/green evidence
Given new conformance tests, when implementation is submitted, then RED and GREEN command evidence is captured.

### AC-2 verification package completeness
Given final diff, when quality gates run, then mutation and live validation results are attached with no tier-matrix blanks.

## Conformance Cases
- C-01 (AC-1): RED failing test command/output captured.
- C-02 (AC-1): GREEN passing conformance command/output captured.
- C-03 (AC-2): mutation + live validation evidence included in PR.

## Success Metrics
- PR contains complete evidence package with no required-section gaps.
