# Spec #2590 - Subtask: package mutation/live-smoke/process evidence for configurable default-importance rollout

Status: Reviewed
Priority: P1
Milestone: M101
Parent: #2589

## Problem Statement
#2589 adds runtime-configurable memory-type default importance behavior; merge gates require reproducible quality, mutation, and live validation evidence.

## Scope
- Re-run #2589 conformance suite.
- Run scoped quality gates.
- Run mutation-in-diff for touched Rust files.
- Run sanitized live validation smoke summary.
- Update issue/process closure artifacts.

## Out of Scope
- Additional feature work beyond #2589 acceptance criteria.

## Acceptance Criteria
- AC-1: #2589 conformance cases C-01..C-05 are reproducibly covered.
- AC-2: Mutation-in-diff reports zero missed mutants (or documents justified N/A if no Rust targets in diff).
- AC-3: Sanitized live smoke completes with zero failures.
- AC-4: Issue logs/checklists/spec statuses updated for closure.

## Conformance Cases
- C-01 (AC-1, conformance): mapped #2589 commands pass
- C-02 (AC-2, mutation): `cargo mutants --in-diff <issue2589-diff>`
- C-03 (AC-3, live validation): sanitized `./scripts/dev/provider-live-smoke.sh` summary reports `failed=0`
- C-04 (AC-4, process): issue process logs and closure artifacts are present

## Success Signals
- Configurable default-importance rollout is auditable and reproducible end-to-end.
