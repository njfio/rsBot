# Spec #2521 - Subtask: RED/GREEN + live validation evidence for G13 ReactTool

Status: Implemented

## Problem Statement
Task #2520 requires explicit RED/GREEN proof, targeted verification, and live validation artifacts to satisfy AGENTS merge gates.

## Acceptance Criteria
### AC-1
Given new conformance tests, when run before implementation, then at least one spec-derived test fails (RED).

### AC-2
Given implementation is complete, when rerun, then all scoped conformance tests pass (GREEN).

### AC-3
Given final diff, when mutation + live validation execute, then no escaped mutants in touched critical paths and live validation passes.

## Conformance Cases
- C-01: RED evidence command/output for a spec_2520 test.
- C-02: GREEN evidence for all spec_2520 conformance tests.
- C-03: `cargo mutants --in-diff` (scoped) shows zero escapes.
- C-04: `./scripts/demo/local.sh ...` passes.
