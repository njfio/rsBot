# Spec #2063

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2063

## Problem Statement

`crates/tau-tools/src/tools.rs` is still above the M25 threshold and must be
decomposed below 3000 LOC while preserving tool behavior, policy gate
semantics, and runtime integration contracts.

## Acceptance Criteria

- AC-1: `tools.rs` is reduced below 3000 LOC via domain extraction aligned to
  split-map artifacts from `#2062`.
- AC-2: Tool execution and policy/approval gate behavior remains stable after
  extraction.
- AC-3: Unit/functional/integration/regression evidence is posted for the
  decomposition wave.

## Scope

In:

- Execute phased module extraction for tools runtime code.
- Add/update split guardrail checks for `tools.rs` threshold and module
  boundaries.
- Capture parity validation evidence.

Out:

- Decomposition work for `github_issues_runtime.rs` or
  `channel_store_admin.rs`.

## Conformance Cases

- C-01 (AC-1): line-count evidence and guardrail checks show
  `tools.rs < 3000`.
- C-02 (AC-2): targeted `tau-tools` conformance/regression tests remain green
  after extraction.
- C-03 (AC-3): integration evidence from consuming runtime crate is posted.

## Success Metrics

- Primary tools runtime file remains below threshold with validated parity and
  issue closure evidence.
