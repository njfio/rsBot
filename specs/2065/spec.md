# Spec #2065

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2065

## Problem Statement

`crates/tau-github-issues-runtime/src/github_issues_runtime.rs` remains above
the M25 threshold and must be decomposed below 3000 LOC while preserving
GitHub Issues bridge behavior, reason-code contracts, and runtime integration
flows.

## Acceptance Criteria

- AC-1: `github_issues_runtime.rs` is reduced below 3000 LOC using the approved
  split map from `#2064`.
- AC-2: GitHub Issues bridge behavior and error-envelope semantics remain
  stable after extraction.
- AC-3: Unit/functional/integration/regression evidence is posted for the
  decomposition wave.

## Scope

In:

- Execute phased module extraction for GitHub Issues runtime domains.
- Add/update split guardrail checks for threshold + module boundary markers.
- Capture parity validation evidence.

Out:

- Decomposition work for `channel_store_admin.rs`.

## Conformance Cases

- C-01 (AC-1): line-count evidence and guardrail checks show
  `github_issues_runtime.rs < 3000`.
- C-02 (AC-2): targeted runtime conformance/regression checks remain green
  after extraction.
- C-03 (AC-3): integration evidence from consuming runtime crate is posted.

## Success Metrics

- Primary GitHub Issues runtime file remains below threshold with validated
  parity and issue closure evidence.
