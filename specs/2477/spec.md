# Spec #2477 - RED/GREEN conformance evidence for G17 phase-2 template source fallback

Status: Implemented

## Problem Statement
Phase-2 delivery requires explicit RED/GREEN evidence proving template-source fallback and diagnostics behavior.

## Acceptance Criteria
### AC-1 RED evidence exists
Given C-01..C-03 tests are introduced before implementation, when scoped tests run, then at least one `spec_2476` test fails.

### AC-2 GREEN evidence exists
Given implementation is complete, when scoped tests rerun, then all C-01..C-03 tests pass.

## Scope
In scope:
- RED and GREEN command/output evidence for `spec_2476` tests.

Out of scope:
- Additional behavior beyond #2476.

## Conformance Cases
- C-01 (AC-1): failing pre-implementation `spec_2476` run.
- C-02 (AC-2): passing post-implementation `spec_2476` run.
