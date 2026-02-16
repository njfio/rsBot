# Spec #2055

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2055

## Problem Statement

The generated roadmap status sections drifted from current GitHub state and
needed regeneration and validation.

## Acceptance Criteria

- AC-1: Sync command updates generated status blocks in both roadmap docs.
- AC-2: Post-sync check mode reports no drift.

## Scope

In:

- Execute roadmap sync command and check mode.

Out:

- Script logic changes.

## Conformance Cases

- C-01 (AC-1): sync output lists updated target files.
- C-02 (AC-2): check mode exits 0 with quiet output.

## Success Metrics

- Status blocks show all tracked roadmap waves closed.
