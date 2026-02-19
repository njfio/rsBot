# Spec #2573 - Subtask: conformance/mutation/live-validation evidence for G2 phase-4

Status: Implemented
Priority: P0
Milestone: M98
Parent: #2572

## Problem Statement
Task #2572 delivers compaction-entry persistence and memory extraction/save behavior. AGENTS merge gates require explicit evidence packaging (RED/GREEN, mutation, and live validation) for auditable closure.

## Scope
- Re-run and record #2572 conformance/regression tests.
- Run scoped quality gates (`fmt`, `clippy`, crate tests).
- Run mutation-in-diff on touched phase-4 paths.
- Run sanitized live validation smoke and capture summary.
- Update process logs and checklist artifacts.

## Out of Scope
- Net-new runtime behavior beyond #2572.
- Full paid multi-provider matrix execution.

## Acceptance Criteria
- AC-1: #2572 conformance cases C-01..C-05 pass and are recorded.
- AC-2: Mutation-in-diff reports zero missed mutants (or escapes are resolved before closure).
- AC-3: Sanitized live smoke completes with zero failures.
- AC-4: Evidence artifacts and process logs are updated for closure.

## Conformance Cases
- C-01 (AC-1, conformance): `cargo test -p tau-agent-core spec_2572_`
- C-02 (AC-1, regression): `cargo test -p tau-agent-core regression_spec_2572_`
- C-03 (AC-2, mutation): `cargo mutants --in-diff <phase4-diff> -p tau-agent-core`
- C-04 (AC-3, live validation): sanitized `./scripts/dev/provider-live-smoke.sh` summary reports `failed=0`
- C-05 (AC-4, process): issue logs and `tasks/spacebot-comparison.md` updated for phase-4 slice

## Success Signals
- Evidence package is reproducible from commands in this spec.
- No AGENTS verification gaps remain for phase-4 closure.
