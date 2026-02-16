# Spec #2061

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2061

## Problem Statement

`crates/tau-trainer/src/benchmark_artifact.rs` must be decomposed below the M25
threshold while preserving benchmark artifact behavior, serialization contracts,
and reporting/conformance flows.

## Acceptance Criteria

- AC-1: `benchmark_artifact.rs` is reduced below 3000 LOC through modular
  extraction aligned with approved split map (`#2060`).
- AC-2: Benchmark artifact conformance and trainer integration behavior remain
  green after extraction.
- AC-3: Unit/functional/integration/regression evidence is posted for the
  decomposition wave.

## Scope

In:

- Execute phased module extraction for benchmark artifact code.
- Update split guardrail and conformance tests for new thresholds/module
  boundaries.
- Capture validation evidence.

Out:

- Decomposition work for `tools.rs`, `github_issues_runtime.rs`, or
  `channel_store_admin.rs`.

## Conformance Cases

- C-01 (AC-1): line-count evidence shows `benchmark_artifact.rs < 3000`.
- C-02 (AC-2): benchmark artifact conformance tests remain green after split.
- C-03 (AC-3): regression/contract suites pass and are recorded in issue
  evidence.

## Success Metrics

- Primary benchmark artifact file remains under threshold with validated parity.
