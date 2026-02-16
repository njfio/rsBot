# Spec #2139

Status: Implemented
Milestone: specs/milestones/m32/index.md
Issue: https://github.com/njfio/Tau/issues/2139

## Problem Statement

Several startup split helper modules still expose public APIs without rustdoc
markers, and the split-module guard script does not assert marker presence for
these files.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for key public APIs in:
  - `crates/tau-startup/src/startup_model_catalog.rs`
  - `crates/tau-startup/src/startup_multi_channel_adapters.rs`
  - `crates/tau-startup/src/startup_multi_channel_commands.rs`
  - `crates/tau-startup/src/startup_rpc_capabilities_command.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-5 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crates.

## Scope

In:

- rustdoc additions for wave-5 files above
- guard script assertion expansion for wave-5 files
- scoped compile/test verification for `tau-startup`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): all four wave-5 files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-startup --target-dir target-fast` passes.
- C-04 (AC-3, integration): targeted startup tests pass.

## Success Metrics

- Subtask `#2139` merges with bounded docs + guard updates.
- Wave-5 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
