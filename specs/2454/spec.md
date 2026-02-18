# Spec #2454 - implement G7 decay/prune/orphan lifecycle maintenance

Status: Implemented
Milestone: specs/milestones/m77/index.md
Issue: https://github.com/njfio/Tau/issues/2454

## Problem Statement

Memory records currently accumulate without lifecycle maintenance, causing stale
or low-value records to persist indefinitely even with phase-1 metadata in
place.

## Scope

In scope:

- Add maintenance policy and execution API in `tau-memory`.
- Apply decay for stale non-identity records.
- Soft-prune records below importance floor.
- Soft-clean orphan low-importance records.
- Add conformance/regression coverage.

Out of scope:

- Heartbeat scheduler trigger wiring.
- Duplicate detection/merge behavior.

## Acceptance Criteria

- AC-1: Runtime exposes deterministic lifecycle maintenance API + summary.
- AC-2: Non-identity stale memories decay according to policy.
- AC-3: Records below prune floor become forgotten (soft-delete).
- AC-4: Low-importance orphan records become forgotten while linked records are retained.
- AC-5: Conformance/regression tests cover active and forgotten lifecycle paths.

## Conformance Cases

- C-01 (AC-1, unit): maintenance policy defaults + summary counters are deterministic.
- C-02 (AC-2, integration): stale non-identity record decays; identity record does not.
- C-03 (AC-3, functional): below-floor record is forgotten and excluded from default read/list/search.
- C-04 (AC-4, integration): orphan low-importance record forgotten; graph-linked low-importance record retained.
- C-05 (AC-5, regression): above-floor/non-stale record remains active.
