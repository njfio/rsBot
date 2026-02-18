# Spec #2456 - RED/GREEN conformance for G7 phase-2 lifecycle maintenance

Status: Implemented
Milestone: specs/milestones/m77/index.md
Issue: https://github.com/njfio/Tau/issues/2456

## Problem Statement

Phase-2 lifecycle maintenance must land with deterministic conformance tests to
avoid silent over-pruning or stale-record drift regressions.

## Scope

In scope:

- RED/GREEN conformance tests C-01..C-04 for decay/prune/orphan.
- Regression test for identity exemption and active-record retention.

Out of scope:

- Implementation beyond parent task #2455.

## Acceptance Criteria

- AC-1: RED tests fail before maintenance implementation.
- AC-2: GREEN tests pass after maintenance implementation.
- AC-3: Regression proves identity records remain active.

## Conformance Cases

- C-01 (AC-1/AC-2): policy defaults + summary counters.
- C-02 (AC-1/AC-2): decay + prune behavior.
- C-03 (AC-1/AC-2): orphan cleanup behavior.
- C-04 (AC-3): identity exemption regression.
