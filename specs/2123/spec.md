# Spec #2123

Status: Implemented
Milestone: specs/milestones/m30/index.md
Issue: https://github.com/njfio/Tau/issues/2123

## Problem Statement

Several split RPC and GitHub issue helper modules still expose public APIs
without rustdoc markers, and the existing split-module guard script does not
assert marker presence for these files.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for public helper APIs in:
  - `crates/tau-runtime/src/rpc_capabilities_runtime.rs`
  - `crates/tau-runtime/src/rpc_protocol_runtime/transport.rs`
  - `crates/tau-github-issues/src/issue_session_helpers.rs`
  - `crates/tau-github-issues/src/issue_prompt_helpers.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-3 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crates.

## Scope

In:

- rustdoc additions for wave-3 files above
- guard script assertion expansion for wave-3 files
- scoped compile/test verification for `tau-runtime` and `tau-github-issues`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): all four wave-3 files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-runtime --target-dir target-fast` and
  `cargo check -p tau-github-issues --target-dir target-fast` pass.
- C-04 (AC-3, integration): targeted tests for touched modules pass.

## Success Metrics

- Subtask `#2123` merges with bounded docs + guard updates.
- Wave-3 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
