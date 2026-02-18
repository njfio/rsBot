# Spec #2542 - Subtask: RED/GREEN + validation evidence for #2541

Status: Implemented

## Problem Statement
Task #2541 requires explicit RED/GREEN proof and full verification artifacts to satisfy AGENTS merge gates.

## Acceptance Criteria
### AC-1 RED evidence is captured
Given C-01..C-04 tests, when run before implementation, then failures are captured with command/output excerpts.

### AC-2 GREEN evidence is captured
Given implementation is complete, when C-01..C-04 tests and verification gates run, then all targeted checks pass and evidence is attached.

### AC-3 Validation matrix is complete
Given PR readiness checks, when final evidence is assembled, then test-tier matrix, mutation result, and live validation summary are complete with no blank gates.

## Conformance Cases
- C-01 (AC-1): RED command/output excerpt includes C-01..C-04 failures.
- C-02 (AC-2): GREEN command/output excerpt includes C-01..C-04 pass.
- C-03 (AC-3): tier/mutation/live evidence included in PR body.

## Verification Notes
- Full workspace `cargo test` gate is currently blocked by existing `tau-runtime` heartbeat hot-reload failures (specs `2465/2487`) reproducible in isolation; scoped `tau-coding-agent` gates and mutation/live validation for `#2541` pass.
