# Spec #2498 - RED/GREEN validation for G9 phase-2 ingestion worker slice

Status: Accepted

## Problem Statement
Task #2497 requires explicit RED/GREEN proof that phase-2 conformance tests fail before implementation and pass after implementation.

## Acceptance Criteria
### AC-1 RED evidence captured
Given C-01..C-04 tests are added before implementation, when scoped tests run, then at least one conformance test fails.

### AC-2 GREEN evidence captured
Given #2497 implementation is complete, when the same scoped tests rerun, then all C-01..C-04 pass.

## Scope
In scope:
- RED/GREEN command evidence for `spec_2497` test set.

Out of scope:
- Additional behavior beyond #2497 implementation scope.

## Conformance Cases
- C-01 (AC-1): pre-implementation scoped run fails.
- C-02 (AC-2): post-implementation scoped run passes.
