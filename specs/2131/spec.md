# Spec #2131

Status: Implemented
Milestone: specs/milestones/m31/index.md
Issue: https://github.com/njfio/Tau/issues/2131

## Problem Statement

Several wave-4 split runtime/helper modules still expose public APIs without
rustdoc markers, and the split-module guard script does not assert marker
presence for these files.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for key public APIs in:
  - `crates/tau-runtime/src/rpc_protocol_runtime/dispatch.rs`
  - `crates/tau-runtime/src/rpc_protocol_runtime/parsing.rs`
  - `crates/tau-runtime/src/runtime_output_runtime.rs`
  - `crates/tau-github-issues/src/issue_run_error_comment.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-4 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crates.

## Scope

In:

- rustdoc additions for wave-4 files above
- guard script assertion expansion for wave-4 files
- scoped compile/test verification for `tau-runtime` and `tau-github-issues`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): all four wave-4 files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-runtime --target-dir target-fast` and
  `cargo check -p tau-github-issues --target-dir target-fast` pass.
- C-04 (AC-3, integration): targeted tests for touched modules pass.

## Success Metrics

- Subtask `#2131` merges with bounded docs + guard updates.
- Wave-4 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
