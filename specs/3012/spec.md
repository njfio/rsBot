# Spec: Issue #3012 - Correct Review #31 stale crate references

Status: Implemented

## Problem Statement
`tasks/tau-gaps-issues-improvements.md` currently references `tau-context` and `tau-embedding-engine`, which are not present in this repository at HEAD. The document must be corrected and guarded against recurrence.

## Acceptance Criteria

### AC-1 Review #31 under-tested crate table uses valid crate references
Given the under-tested areas section,
When reviewing crate rows,
Then referenced crates correspond to crates present under `crates/` at HEAD.

### AC-2 Stale crate names are explicitly rejected by conformance checks
Given `scripts/dev/test-tau-gaps-issues-improvements.sh`,
When stale crate names (`tau-context`, `tau-embedding-engine`) appear,
Then the script fails.

### AC-3 Conformance and baseline checks pass
Given corrected docs and guard script,
When running the conformance test and baseline checks,
Then all commands pass.

## Scope

### In Scope
- `tasks/tau-gaps-issues-improvements.md`
- `scripts/dev/test-tau-gaps-issues-improvements.sh`
- `specs/milestones/m181/index.md`
- `specs/3012/*`

### Out of Scope
- Runtime code changes.
- Broad rewrite of Review #31.

## Conformance Cases
- C-01: under-tested crate rows reference existing crates only.
- C-02: conformance script fails if stale crate names reappear.
- C-03: `bash scripts/dev/test-tau-gaps-issues-improvements.sh` passes.
- C-04: `cargo fmt --check` and `cargo check -q` pass.

## Success Metrics / Observable Signals
- `bash scripts/dev/test-tau-gaps-issues-improvements.sh`
- `cargo fmt --check`
- `cargo check -q`

## Approval Gate
P2 scope: agent-authored spec, self-reviewed, implementation proceeds with human review in PR.
