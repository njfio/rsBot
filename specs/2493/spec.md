# Spec #2493 - RED/GREEN conformance evidence for G9 phase-1 ingestion

Status: Implemented

## Problem Statement
Task #2492 requires explicit RED/GREEN proof that conformance tests fail before implementation and pass after implementation.

## Acceptance Criteria
### AC-1 RED evidence captured
Given C-01..C-04 tests are added before implementation, when scoped tests run, then at least one conformance test fails.

### AC-2 GREEN evidence captured
Given #2492 implementation is complete, when same scoped tests rerun, then all C-01..C-04 pass.

## Scope
In scope:
- RED/GREEN command evidence for `spec_2492` test set.

Out of scope:
- Additional behavior beyond #2492 implementation scope.

## Conformance Cases
- C-01 (AC-1): pre-implementation scoped run fails.
- C-02 (AC-2): post-implementation scoped run passes.
