# Spec #2585 - Subtask: package mutation/live-smoke/process evidence for G5-G7 closure audit

Status: Implemented
Priority: P1
Milestone: M100
Parent: #2584

## Problem Statement
Task #2584 validates G5/G6/G7 parity and updates roadmap checklists. AGENTS merge gates still require reproducible quality/mutation/live evidence and process artifact closure.

## Scope
- Re-run mapped #2584 conformance tests.
- Run scoped lint/format and relevant crate test gates.
- Run mutation-in-diff for touched paths.
- Run sanitized live smoke summary.
- Update process/closure artifacts.

## Out of Scope
- New feature architecture work beyond checklist parity validation.

## Acceptance Criteria
- AC-1: #2584 conformance cases C-01..C-06 are reproducibly covered.
- AC-2: Mutation-in-diff reports zero missed mutants, or documents a docs-only/N/A outcome when no Rust sources exist in diff.
- AC-3: Sanitized live smoke completes with zero failures.
- AC-4: Issue logs/checklists are updated for closure.

## Conformance Cases
- C-01 (AC-1, conformance): mapped #2584 test commands pass
- C-02 (AC-2, mutation): `cargo mutants --in-diff <g5-g7-diff>` passes with zero missed, or returns `Diff changes no Rust source files` for docs-only diff
- C-03 (AC-3, live validation): sanitized `./scripts/dev/provider-live-smoke.sh` summary reports `failed=0`
- C-04 (AC-4, process): issue process logs and roadmap checklist updates are present

## Success Signals
- Closure package is auditable and reproducible.
