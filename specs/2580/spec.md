# Spec #2580 - Subtask: conformance/mutation/live-validation evidence for G2 phase-5 warn-tier LLM compaction

Status: Implemented
Priority: P0
Milestone: M99
Parent: #2579

## Problem Statement
Task #2579 introduces warn-tier background LLM summarization behavior. AGENTS merge gates require reproducible conformance, scoped quality, mutation, and live-validation evidence before closure.

## Scope
- Re-run and record #2579 conformance/regression tests.
- Run scoped quality gates (`fmt`, `clippy`, crate tests).
- Run mutation-in-diff for touched phase-5 paths.
- Run sanitized live validation smoke and capture summary.
- Update process logs/checklists for closure.

## Out of Scope
- Net-new runtime behavior beyond #2579.
- Full paid multi-provider matrix execution.

## Acceptance Criteria
- AC-1: #2579 conformance/regression cases C-01..C-05 pass and are recorded.
- AC-2: Mutation-in-diff reports zero missed mutants (or escapes are resolved before closure).
- AC-3: Sanitized live smoke completes with zero failures.
- AC-4: Evidence artifacts and process logs/checklists are updated for closure.

## Conformance Cases
- C-01 (AC-1, conformance): `cargo test -p tau-agent-core spec_2579_`
- C-02 (AC-1, regression): `cargo test -p tau-agent-core regression_spec_2579_`
- C-03 (AC-2, mutation): `cargo mutants --in-diff <phase5-diff> -p tau-agent-core`
- C-04 (AC-3, live validation): sanitized `./scripts/dev/provider-live-smoke.sh` summary reports `failed=0`
- C-05 (AC-4, process): issue logs and `tasks/spacebot-comparison.md` updated for phase-5 slice

## Success Signals
- Evidence package is reproducible via listed commands.
- No AGENTS verification gap remains for #2579 closure.
