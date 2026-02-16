# Spec #2047

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2047

## Problem Statement

M25.4.3 requires task-level confirmation that CI cache and helper-suite
parallelization optimizations were delivered without reducing validation
coverage or introducing flaky behavior. Subtask `#2070` implemented the
workflow and artifact changes; this task closes the parent by mapping those
deliverables to acceptance criteria and validated evidence.

## Acceptance Criteria

- AC-1: CI workflow includes lane-scoped cache shared keys for Linux quality,
  WASM smoke, cross-platform smoke, and coverage lanes.
- AC-2: Helper-suite scheduling runs in parallel while preserving discovery
  scope/pattern for `.github/scripts/test_*.py`.
- AC-3: Timing evidence and verification suites demonstrate improvement signals
  and no increase in failing/flaky helper checks.

## Scope

In:

- Consume merged `#2070` changes and publish task-level conformance evidence.
- Verify workflow contract and helper-runner suites against current `master`.
- Close parent task `#2047` with links to artifacts/tests.

Out:

- Re-implement subtask code already delivered in `#2070`.
- Baseline-only work from `#2045`/`#2068`.
- Latency budget policy enforcement from `#2048`/`#2071`.

## Conformance Cases

- C-01 (AC-1, integration): `.github/workflows/ci.yml` contains shared-key
  snippets for `ci-quality-linux-`, `ci-wasm-smoke-`, `ci-cross-platform-`,
  and `ci-coverage-`.
- C-02 (AC-2, functional): helper validation step uses
  `ci_helper_parallel_runner.py --workers 4 --start-dir .github/scripts --pattern "test_*.py"`.
- C-03 (AC-3, functional): `tasks/reports/m25-ci-cache-parallel-tuning.{json,md}`
  exists with serial/parallel medians and improvement status.
- C-04 (AC-3, regression): helper and workflow contract suites pass, including
  fail-closed invalid fixture/runner regressions.

## Success Metrics

- Parent task closes with conformance evidence linked to merged PR `#2082`.
- Workflow contract and helper test suites pass on latest `master`.
- Timing report artifact remains present with measurable improvement status.
