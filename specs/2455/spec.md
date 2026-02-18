# Spec #2455 - add G7 phase-2 lifecycle maintenance API in tau-memory

Status: Implemented
Milestone: specs/milestones/m77/index.md
Issue: https://github.com/njfio/Tau/issues/2455

## Problem Statement

Phase-1 added lifecycle metadata but lacks an execution path that uses
`last_accessed_at_unix_ms`, `access_count`, and graph relation data to enforce
memory hygiene.

## Scope

In scope:

- Add maintenance policy contract (decay/prune/orphan thresholds).
- Add runtime maintenance function that mutates records via append-only writes.
- Keep soft-delete semantics (`forgotten`) as the only deletion mode.
- Ensure identity memories are decay/prune exempt.

Out of scope:

- Scheduler/heartbeat integration.
- Embedding-based duplicate detection.

## Acceptance Criteria

- AC-1: Given a policy and current time, when maintenance runs, then it returns deterministic counters for scanned/decayed/pruned/orphan-forgotten records.
- AC-2: Given stale non-identity records, when maintenance runs, then importance decays by configured rate.
- AC-3: Given decayed or pre-existing low-importance records below prune floor, when maintenance runs, then records are soft-deleted (`forgotten=true`).
- AC-4: Given low-importance orphan records with no inbound/outbound edges, when maintenance runs, then orphan records are soft-deleted while edge-connected records are retained.
- AC-5: Given identity records, when maintenance runs, then decay/prune/orphan rules do not soft-delete them.

## Conformance Cases

- C-01 (AC-1, unit): policy defaults + maintenance summary counters.
- C-02 (AC-2, integration): stale non-identity decay behavior.
- C-03 (AC-3, functional): prune-floor forgotten behavior.
- C-04 (AC-4, integration): orphan cleanup respects graph edge presence.
- C-05 (AC-5, regression): identity record remains active.

## Success Metrics / Observable Signals

- C-01..C-05 pass in `tau-memory` test suite.
- `cargo fmt --check` and scoped `clippy` pass.
- No regression in existing lifecycle phase-1 tests.
