# Spec #2051

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2051

## Problem Statement

The lint command existed but did not enforce namespace-label contract fields,
causing incomplete governance validation.

## Acceptance Criteria

- AC-1: Drift checker enforces required label namespace prefixes.
- AC-2: Output diagnostics identify missing prefixes deterministically.
- AC-3: Parent hierarchy compatibility remains enforced.

## Scope

In:

- Extend dependency drift checker logic and fixtures.

Out:

- Broader issue-management automation.

## Conformance Cases

- C-01 (AC-1): Namespace-prefix check path executes from policy settings.
- C-02 (AC-2): Missing-prefix findings include `drift.missing_required_label_prefixes`.
- C-03 (AC-3): Existing parent-link and parent-type checks still pass fixtures.

## Success Metrics

- `scripts/dev/test-dependency-drift-check.sh` passes with updated policy/fixtures.
