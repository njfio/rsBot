# Spec #2044

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2044

## Problem Statement

`crates/tau-ops/src/channel_store_admin.rs` remained oversized for M25.3 and
needed decomposition below 2200 LOC while preserving channel-store admin status
inspection, operator-control summary, and transport-health command behavior.

## Acceptance Criteria

- AC-1: Primary file is reduced below 2200 LOC.
- AC-2: Channel-store admin unit/functional/integration/regression behavior
  remains green after decomposition.

## Scope

In:

- Define and validate split-map plan (`#2066`).
- Execute operator-control domain extraction and parity checks (`#2067`).

Out:

- Further decomposition beyond threshold target.

## Conformance Cases

- C-01 (AC-1): guardrail proves `channel_store_admin.rs < 2200`.
- C-02 (AC-2): targeted operator summary and snapshot roundtrip tests pass.
- C-03 (AC-2): regression handling for missing state files remains fail-closed
  and deterministic.

## Success Metrics

- M25.3.5 threshold is met and parity evidence captured in subtask closures.
