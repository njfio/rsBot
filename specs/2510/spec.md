# Spec #2510 - RED/GREEN + live validation evidence for G11 closure

Status: Implemented

## Problem Statement
Task #2509 needs explicit RED/GREEN and live validation proof before closure.

## Acceptance Criteria
### AC-1 RED evidence captured
Given C-01..C-04 tests are written before implementation, when scoped tests run, then at least one conformance test fails.

### AC-2 GREEN evidence captured
Given implementation is complete, when scoped tests rerun, then C-01..C-04 pass.

### AC-3 Live validation evidence captured
Given feature implementation is complete, when live validation workflow runs, then no regressions are observed and evidence is attached.

## Scope
In scope:
- RED/GREEN command evidence for `spec_2509`.
- Live validation command/output summary.

Out of scope:
- Additional behavior beyond #2509.

## Conformance Cases
- C-01 (AC-1): pre-implementation scoped run fails.
- C-02 (AC-2): post-implementation scoped run passes.
- C-03 (AC-3): live validation command succeeds.
