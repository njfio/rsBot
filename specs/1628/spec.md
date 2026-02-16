# Issue 1628 Spec

Status: Implemented

Issue: `#1628`  
Milestone: `#21`  
Parent: `#1611`

## Problem Statement

Training micro-crate boundary decisions were documented as retain-first, but
the consolidation follow-through set for `#1628` is still marked planned.
This leaves ownership resolution ambiguous for execution governance.

## Scope

In scope:

- finalize training micro-crate decision status for `#1628` as explicit retention
- regenerate machine-readable boundary-plan artifacts with completed status
- update boundary docs to reflect completed retention decision
- verify training crate path compiles/tests pass under retained boundaries

Out of scope:

- merging training crates into fewer crates
- changing public APIs or crate names
- adding dependencies

## Acceptance Criteria

AC-1 (completed retention status):
Given training boundary plan artifacts,
when generated for this issue,
then `training-boundary-set-c` is marked completed for `#1628` with explicit
retention scope.

AC-2 (no ownership ambiguity):
Given generated training boundary plan JSON,
when validated,
then ambiguous decision count is zero and all crates have explicit decision and
owner surface text.

AC-3 (docs alignment):
Given boundary guide docs,
when reviewed,
then they state retained split ownership and `#1628` completion status.

AC-4 (compile/test verification):
Given retained training crate boundaries,
when scoped tests run,
then training path compiles and targeted crate tests pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given generator output, when reading first PR sets, then set-c status is `completed` and references `#1628`. |
| C-02 | AC-2 | Regression | Given generator output, when validated, then ambiguous count is 0 and owner_surface fields are non-empty. |
| C-03 | AC-3 | Functional | Given `docs/guides/training-crate-boundary-plan.md`, when read, then completion status for set-c is explicit. |
| C-04 | AC-4 | Integration | Given retained split, when running targeted training crate tests, then commands pass. |

## Success Metrics

- no unresolved boundary status remains for training micro-crates in M21 lane
- retained ownership boundaries are explicit and reproducible
