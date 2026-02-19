# Spec #2567 - Subtask: conformance/mutation/live-validation evidence for G2 phase-3 background compaction

Status: Reviewed
Priority: P0
Milestone: M97
Parent: #2566

## Problem Statement
Task #2566 adds warn-tier background compaction orchestration. Merge gates for this phase require explicit, reproducible evidence for conformance mapping, mutation quality, and live validation outcomes.

## Scope
- Re-run and record mapped #2566 conformance/regression tests.
- Run scoped quality gates (`fmt`, `clippy`, crate tests).
- Run mutation-in-diff for touched phase-3 paths.
- Run live validation smoke using sanitized/local-safe flow and capture summary.
- Update issue process logs and comparison checklist status for this phase.

## Out of Scope
- Net-new runtime behavior beyond #2566 implementation.
- Full paid multi-provider matrix runs.
- Broader non-phase-3 backlog work.

## Acceptance Criteria
- AC-1: Given #2566 conformance cases C-01..C-05, when verification runs, then all mapped tests pass and are recorded.
- AC-2: Given touched phase-3 diff, when mutation testing runs, then escaped mutants are zero or resolved before closure.
- AC-3: Given local-safe live validation smoke flow, when executed, then it completes with zero failures.
- AC-4: Given evidence is complete, when process artifacts are updated, then issue logs and `tasks/spacebot-comparison.md` phase status reflect completion.

## Conformance Cases
- C-01 (AC-1, conformance): `cargo test -p tau-agent-core spec_2566_`
- C-02 (AC-1, regression): `cargo test -p tau-agent-core regression_spec_2566_`
- C-03 (AC-2, mutation): `cargo mutants --in-diff <phase3-diff> -p tau-agent-core`
- C-04 (AC-3, live validation): sanitized `./scripts/dev/provider-live-smoke.sh` reports `failed=0`
- C-05 (AC-4, process): #2566/#2567 process logs + `tasks/spacebot-comparison.md` updated for delivered slice

## Success Signals
- Evidence package is reproducible from commands listed in this spec.
- No missing AGENTS contract gates remain for phase-3 closure.
