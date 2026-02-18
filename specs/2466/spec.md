# Spec #2466 - RED/GREEN conformance for G16 phase-1 hot-reload behavior

Status: Implemented

## Problem Statement
G16 phase-1 requires explicit RED/GREEN proof for heartbeat hot-reload conformance before closure.

## Acceptance Criteria
### AC-1 RED evidence is captured for conformance tests
Given C-01..C-03 tests, when executed before implementation, then at least one conformance test fails due to missing hot-reload behavior.

### AC-2 GREEN evidence is captured after implementation
Given implementation complete, when C-01..C-03 tests run, then all pass and map to #2465 AC matrix.

## Scope
In scope:
- RED and GREEN execution evidence for C-01..C-03.
- Test-name linkage in PR evidence.

Out of scope:
- New functionality beyond #2465 scope.

## Conformance Cases
- C-01 (AC-1): failing RED run on hot-reload conformance tests.
- C-02 (AC-2): passing GREEN run on hot-reload conformance tests.
