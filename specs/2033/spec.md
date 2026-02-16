# Spec #2033

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2033

## Problem Statement

M25.4 is the story-level velocity wave for reducing build/test feedback latency
without sacrificing reliability. Child tasks (`#2045`, `#2046`, `#2047`,
`#2048`) have delivered baseline capture, optimization paths, and budget
enforcement. This story closes when those deliverables are mapped to the story
acceptance criteria and verified as a coherent end-to-end pipeline.

## Acceptance Criteria

- AC-1: Baseline timings are captured and versioned in durable report artifacts.
- AC-2: Optimization work improves selected critical-path commands via
  fast-lane and CI cache/parallel execution paths.
- AC-3: Regression policy and gate checks block unacceptable latency drift.

## Scope

In:

- Story-level roll-up validation of completed child tasks:
  - `#2045` baseline timing artifacts
  - `#2046` fast-lane optimization artifacts
  - `#2047` CI cache + helper parallel optimization artifacts
  - `#2048` latency budget policy/gate artifacts
- Consolidated conformance mapping from ACs to child deliverables/tests.
- Story closure with verified evidence links.

Out:

- New optimization implementation outside delivered child tasks.
- Re-baselining unrelated milestones or architecture redesign.

## Conformance Cases

- C-01 (AC-1, functional): baseline artifacts
  `tasks/reports/m25-build-test-latency-baseline.{json,md}` exist and baseline
  contract suite passes.
- C-02 (AC-2, integration): optimization artifacts show improved status for
  fast-lane and CI helper parallel tuning:
  `tasks/reports/m25-fast-lane-loop-comparison.json`,
  `tasks/reports/m25-ci-cache-parallel-tuning.json`.
- C-03 (AC-3, functional): latency budget policy and gate artifacts exist and
  pass contract + shell gate tests:
  `tasks/policies/m25-latency-budget-policy.json`,
  `tasks/reports/m25-latency-budget-gate.{json,md}`.
- C-04 (AC-1..AC-3, regression): consolidated story verification suites pass
  without introducing failing/flaky helper checks.

## Success Metrics

- Story `#2033` closes with all child tasks `#2045/#2046/#2047/#2048` closed
  and `status:done`.
- All mapped shell + Python contract suites pass on latest `master`.
- Report/policy artifacts remain versioned and discoverable under
  `tasks/reports/` and `tasks/policies/`.
