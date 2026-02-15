# Issue 1674 Spec

Status: Implemented

Issue: `#1674`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

M24 benchmark decisions currently rely on raw averages and reproducibility bands,
but lack explicit statistical confidence summaries. We need deterministic
mean/variance/confidence reporting and baseline-vs-candidate comparison artifacts
that can be consumed by operators and automation.

## Scope

In scope:

- add summary statistics for benchmark score vectors (count, mean, variance,
  stddev, confidence interval)
- add baseline-vs-candidate comparison reporting with confidence interval on
  improvement delta
- expose machine-readable report output in `tau-trainer`
- add conformance/regression tests for deterministic statistics contracts

Out of scope:

- live-run execution protocol (`#1698`)
- benchmark fixture generation (`#1697`)
- dashboard rendering or external storage/publishing pipeline

## Acceptance Criteria

AC-1 (summary statistics):
Given a score sample vector,
when statistics are computed,
then mean/variance/stddev and confidence interval are emitted deterministically.

AC-2 (comparative significance):
Given baseline and candidate sample vectors,
when comparison is computed,
then improvement delta with confidence interval and significance decision is
reported.

AC-3 (machine-readable report):
Given computed comparison output,
when serialized,
then report structure is machine-readable and includes all required metrics.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given deterministic sample vectors, when summary stats are computed, then expected count/mean/variance/CI metrics match reference values. |
| C-02 | AC-2 | Functional | Given baseline/candidate vectors with clear lift, when comparison runs, then delta CI and significance decision reflect improvement. |
| C-03 | AC-3 | Regression | Given comparison report serialization, when converted to JSON, then required keys/values are present and stable. |
| C-04 | AC-1/AC-2 | Regression | Given empty or non-finite sample input, when stats/comparison run, then deterministic validation errors are returned. |

## Success Metrics

- policy-improvement claims include confidence metrics, not only means
- reports can be consumed by scripts as stable JSON objects
- invalid inputs fail fast with actionable diagnostics
