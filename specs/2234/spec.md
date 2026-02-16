# Spec #2234

Status: Implemented
Milestone: specs/milestones/m44/index.md
Issue: https://github.com/njfio/Tau/issues/2234

## Problem Statement

Task #2234 must deliver the concrete `tau-ai` OpenAI client changes required to
support Codex models with endpoint-compatible request/response behavior and
test-backed validation.

## Acceptance Criteria

- AC-1: Subtask `#2235` is implemented and closed with `status:done`.
- AC-2: `tau-ai` OpenAI client supports Responses API routing/parsing for
  Codex requests.
- AC-3: Conformance/integration tests and scoped quality gates pass.

## Conformance Cases

- C-01 (AC-1): `#2235` closed with completion metadata.
- C-02 (AC-2): direct Codex route + fallback behavior covered in tests.
- C-03 (AC-3): `fmt`/`clippy`/`cargo test -p tau-ai` pass.
