# Spec #2067

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2067

## Problem Statement

`crates/tau-ops/src/channel_store_admin.rs` exceeded the M25.3.5 budget and
needed decomposition below 2200 LOC while preserving channel-store admin CLI
behavior, operator-control summary/diff semantics, and existing test coverage.

## Acceptance Criteria

- AC-1: `channel_store_admin.rs` is reduced below 2200 LOC by extracting at
  least one high-volume domain module from the approved split map (`#2066`).
- AC-2: Operator-control summary/snapshot/diff behavior remains stable after
  extraction.
- AC-3: Unit + functional + integration + regression proof is captured for the
  extracted domain.

## Scope

In:

- Extract operator-control summary/diff helpers into a focused module.
- Wire module imports without changing `execute_channel_store_admin_command`
  entrypoint behavior.
- Tighten domain split guardrail to the M25.3.5 threshold.

Out:

- Additional optional extraction phases beyond threshold compliance.

## Conformance Cases

- C-01 (AC-1): guardrail confirms `channel_store_admin.rs < 2200` and required
  module markers/files exist.
- C-02 (AC-2): operator summary + snapshot/diff behavior tests stay green after
  extraction.
- C-03 (AC-3): targeted unit/functional/integration/regression tests pass with
  no behavior drift.

## Success Metrics

- Channel-store admin decomposition lands below threshold with parity evidence
  and issue closure.
