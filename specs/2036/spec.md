# Spec #2036

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2036

## Problem Statement

Milestone spec-container coverage was incomplete (only m21-m25 existed locally),
while repository milestones extend across earlier waves.

## Acceptance Criteria

- AC-1: Milestone coverage inventory is produced with explicit covered/missing counts.
- AC-2: Missing milestone spec index files are backfilled or explicitly waived.
- AC-3: Coverage report is committed as audit artifact.

## Scope

In:

- Inventory all GitHub milestones (`state=all`) and local `specs/milestones`.
- Backfill missing `specs/milestones/m<number>/index.md`.
- Publish coverage report artifacts.

Out:

- Refactoring historical milestone issue hierarchies.

## Conformance Cases

- C-01 (AC-1, functional): coverage artifact reports all milestones and local file presence.
- C-02 (AC-2, functional): missing milestone indices are generated.
- C-03 (AC-3, regression): post-backfill coverage reports `missing=0`.

## Success Metrics

- Coverage report shows `covered=25`, `missing=0`.
