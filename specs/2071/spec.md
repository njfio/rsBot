# Spec #2071

Status: Accepted
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2071

## Problem Statement

M25.4.4 needs explicit latency-budget thresholds and deterministic enforcement
checks so regressions are surfaced with actionable diagnostics instead of manual
inspection. Current artifacts capture baseline and fast-lane comparisons, but
they do not define pass/fail policy gates.

## Acceptance Criteria

- AC-1: Latency-budget policy thresholds are codified in a machine-readable
  policy artifact and documented for operators.
- AC-2: A gating script evaluates benchmark reports against policy thresholds
  and exits non-zero on violations.
- AC-3: Violation output includes actionable diagnostics identifying metric,
  threshold, observed value, and recommended remediation.

## Scope

In:

- Add policy JSON for latency budget thresholds.
- Add check script producing deterministic JSON + Markdown gate artifacts.
- Add shell + Python contract tests for pass/fail and diagnostics behavior.
- Add operator guide for policy and gate execution workflow.

Out:

- CI workflow integration changes (`#2070`/`#2047`) requiring pipeline edits.
- Broader task-level roll-up closure for `#2048`.

## Conformance Cases

- C-01 (AC-1, functional): policy artifact exists with required threshold fields
  and documentation references.
- C-02 (AC-2, integration): gate script returns pass for a compliant fixture and
  fail for a violating fixture.
- C-03 (AC-3, regression): failing gate output includes metric + threshold +
  observed + remediation fields in JSON and Markdown artifacts.
- C-04 (AC-2/AC-3, regression): malformed policy/report inputs fail closed with
  non-zero exit and actionable error text.

## Success Metrics

- Policy + gate tooling are checked in with deterministic test coverage.
- Gate artifacts capture pass/fail decision and actionable diagnostics.
- `#2071` closes with evidence ready for parent task `#2048`.
