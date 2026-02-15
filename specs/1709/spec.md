# Issue 1709 Spec

Status: Implemented

Issue: `#1709`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

M24 needs a reproducible, scriptable way to produce baseline-vs-trained
benchmark significance reports so policy-improvement claims can be verified by
maintainers without manual calculations.

## Scope

In scope:

- add benchmark significance report generator script for baseline/trained sample
  vectors
- emit report artifact compatible with existing benchmark-report validator
- include p-value, confidence, delta CI, and pass/fail fields in output
- add tests for successful generation and fail-closed invalid input paths

Out of scope:

- distributed benchmark execution orchestration
- dashboard visualization
- external storage/DB publication

## Acceptance Criteria

AC-1 (reproducible significance generation):
Given baseline and trained sample vectors,
when report generation executes,
then significance metrics and improvement decision are emitted deterministically.

AC-2 (artifact compatibility):
Given generated significance report artifact,
when benchmark report validator executes,
then artifact passes schema/contract validation.

AC-3 (operator-consumable metrics):
Given generated report,
when inspected,
then p-value, confidence, delta confidence interval, and pass/fail are present.

AC-4 (fail-closed invalid input):
Given malformed or mismatched sample vectors,
when report generation executes,
then command exits non-zero with actionable errors.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given valid baseline/trained sample arrays, when generator runs, then output contains deterministic significance metrics and `pass=true` for clear improvement. |
| C-02 | AC-2 | Integration | Given generated report artifact, when `validate-m24-rl-benchmark-report.sh` runs, then validation succeeds for `report_kind=significance`. |
| C-03 | AC-3 | Functional | Given generated report, when parsed, then `significance.{p_value,confidence_level,mean_delta,delta_ci_low,delta_ci_high,pass}` fields exist. |
| C-04 | AC-4 | Regression | Given mismatched sample lengths or non-finite values, when generator runs, then command fails closed with deterministic error text. |

## Success Metrics

- maintainers can generate baseline-vs-trained significance report with one
  command
- generated artifact is validator-compatible and machine-readable
- invalid inputs are blocked with deterministic diagnostics
