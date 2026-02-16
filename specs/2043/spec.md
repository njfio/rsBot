# Spec #2043

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2043

## Problem Statement

`crates/tau-github-issues-runtime/src/github_issues_runtime.rs` remained above
the M25 decomposition threshold and required phased extraction below 3000 LOC
without breaking GitHub issue runtime behavior.

## Acceptance Criteria

- AC-1: `github_issues_runtime.rs` is reduced below 3000 LOC.
- AC-2: Runtime behavior parity is preserved with targeted test evidence.

## Scope

In:

- Split-map planning (`#2064`) and execution split (`#2065`) for GitHub issues
  runtime.
- Guardrail updates and parity evidence capture.

Out:

- Additional decomposition beyond the threshold target.

## Conformance Cases

- C-01 (AC-1): line-budget guardrail proves
  `github_issues_runtime.rs < 3000`.
- C-02 (AC-2): unit/integration/regression evidence remains green after
  extraction.

## Success Metrics

- M25.3.4 threshold met and validated with split-map + execution subtask proof.
