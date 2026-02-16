# Spec #2187

Status: Implemented
Milestone: specs/milestones/m38/index.md
Issue: https://github.com/njfio/Tau/issues/2187

## Problem Statement

Wave-11 provider auth runtime split helper modules still expose public APIs
without rustdoc markers, and the split-module rustdoc guard does not enforce
marker presence for this module set.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for key public APIs in:
  - `crates/tau-provider/src/auth_commands_runtime/anthropic_backend.rs`
  - `crates/tau-provider/src/auth_commands_runtime/openai_backend.rs`
  - `crates/tau-provider/src/auth_commands_runtime/google_backend.rs`
  - `crates/tau-provider/src/auth_commands_runtime/shared_runtime_core.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-11 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crate.

## Scope

In:

- rustdoc additions for wave-11 provider auth runtime modules listed above
- guard script assertion expansion for wave-11 markers
- scoped compile/test verification for `tau-provider`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): all four wave-11 files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-provider --target-dir target-fast` passes.
- C-04 (AC-3, integration): targeted provider auth runtime tests pass.

## Success Metrics

- Subtask `#2187` merges with bounded docs + guard updates.
- Wave-11 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
