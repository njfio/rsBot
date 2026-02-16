# Spec #2041

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2041

## Problem Statement

`crates/tau-trainer/src/benchmark_artifact.rs` exceeded the M25 decomposition
threshold and needed to be split while preserving benchmark artifact behavior,
schema/reporting contracts, and training-loop integration behavior.

## Acceptance Criteria

- AC-1: `benchmark_artifact.rs` is reduced below 3000 LOC with decomposition
  aligned to the split-map plan.
- AC-2: Benchmark artifact conformance behavior remains stable after the split.
- AC-3: Regression and trainer integration checks remain green after extraction.

## Scope

In:

- Execute M25.3 benchmark artifact split-map planning and execution subtasks.
- Externalize high-volume benchmark artifact test/decomposition domains.
- Capture and post conformance evidence tied to ACs.

Out:

- Decomposition work for `tools.rs`, `github_issues_runtime.rs`, and other
  oversized files outside `benchmark_artifact.rs`.

## Conformance Cases

- C-01 (AC-1): `benchmark_artifact.rs` line count is below 3000 and split
  guardrail passes.
- C-02 (AC-2): `benchmark_artifact::tests::spec_1980` conformance slice passes
  after decomposition.
- C-03 (AC-3): benchmark artifact regression test and trainer `fit()` worker
  integration test pass after extraction.

## Success Metrics

- Primary benchmark artifact file remains below threshold with parity evidence
  posted and parent task closed.
