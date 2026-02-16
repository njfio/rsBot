# Spec #2195

Status: Implemented
Milestone: specs/milestones/m39/index.md
Issue: https://github.com/njfio/Tau/issues/2195

## Problem Statement

Wave-12 GitHub issues runtime split helper modules still expose public APIs
without rustdoc markers, and the split-module rustdoc guard does not enforce
marker presence for this module set.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for key public APIs in:
  - `crates/tau-github-issues-runtime/src/github_issues_runtime/demo_index_runtime.rs`
  - `crates/tau-github-issues-runtime/src/github_issues_runtime/issue_command_rendering.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-12 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crate.

## Scope

In:

- rustdoc additions for wave-12 modules listed above
- guard script assertion expansion for wave-12 markers
- scoped compile/test verification for `tau-github-issues-runtime`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): wave-12 files contain expected rustdoc marker
  phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-github-issues-runtime --target-dir target-fast` passes.
- C-04 (AC-3, integration): targeted GitHub runtime tests pass.

## Success Metrics

- Subtask `#2195` merges with bounded docs + guard updates.
- Wave-12 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
