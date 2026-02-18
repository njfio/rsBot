# Spec #2472 - RED/GREEN conformance for G17 phase-1 startup prompt template rendering

Status: Implemented

## Problem Statement
G17 phase-1 must include explicit RED/GREEN proof for template-rendering conformance before closure.

## Acceptance Criteria
### AC-1 RED evidence is captured
Given C-01..C-03 tests, when executed before implementation, then at least one test fails due to missing template behavior.

### AC-2 GREEN evidence is captured
Given implementation complete, when C-01..C-03 tests execute, then all pass and map to #2471 AC matrix.

## Scope
In scope:
- RED and GREEN command/output evidence for C-01..C-03.

Out of scope:
- Additional behavior beyond #2471.

## Conformance Cases
- C-01 (AC-1): failing RED run for C-01..C-03.
- C-02 (AC-2): passing GREEN run for C-01..C-03.
