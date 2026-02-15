# Issue 1706 Spec

Status: Accepted

Issue: `#1706`  
Milestone: `#22`  
Parent: `#1700`

## Problem Statement

M22 renamed training flags to prompt-optimization terminology. We now need a
reproducible validation artifact proving compatibility aliases still work,
warning text is deterministic, and migration policy is documented for operators.

## Scope

In scope:

- executable validation script for alias compatibility checks
- JSON + Markdown validation artifacts for gate evidence
- documentation of final compatibility policy and migration path

Out of scope:

- introducing new flag aliases beyond current compatibility set
- broad CLI redesign unrelated to M22 naming alignment

## Acceptance Criteria

AC-1 (alias behavior validation):
Given the current CLI implementation,
when compatibility validation runs,
then alias behavior tests execute and pass with deterministic outputs.

AC-2 (warning text validation):
Given legacy alias usage,
when validation runs,
then warning/deprecation message checks are included and reported.

AC-3 (migration policy documentation):
Given operator-facing training docs,
when reviewing compatibility guidance,
then migration policy from legacy aliases to canonical flags is explicit.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given validation script, when run, then alias compatibility test commands execute successfully. |
| C-02 | AC-2 | Functional | Given warning snapshot tests, when run by script, then pass/fail status is captured in artifact. |
| C-03 | AC-1, AC-2 | Conformance | Given successful run, when JSON report is emitted, then required fields include command list, per-test status, and summary counts. |
| C-04 | AC-3 | Integration | Given updated docs, when docs checks run, then compatibility policy guidance is discoverable from docs index/training guide. |
| C-05 | AC-1 | Regression | Given invalid script options, when invoked, then deterministic non-zero errors are produced. |

## Success Metrics

- one-command alias validation report generation for M22 gate
- stable warnings validated by tests and surfaced in artifact summary
- documented migration path from legacy to canonical flag names
