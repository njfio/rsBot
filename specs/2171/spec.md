# Spec #2171

Status: Implemented
Milestone: specs/milestones/m36/index.md
Issue: https://github.com/njfio/Tau/issues/2171

## Problem Statement

Wave-9 session/memory split helper modules still expose public APIs without
rustdoc markers, and the split-module rustdoc guard does not enforce marker
presence for this module set.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for key public APIs in:
  - `crates/tau-session/src/session_locking.rs`
  - `crates/tau-session/src/session_storage.rs`
  - `crates/tau-session/src/session_integrity.rs`
  - `crates/tau-memory/src/runtime/backend.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-9 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crates.

## Scope

In:

- rustdoc additions for wave-9 session/memory modules listed above
- guard script assertion expansion for wave-9 markers
- scoped compile/test verification for `tau-session` and `tau-memory`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): all four wave-9 files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-session --target-dir target-fast` and
  `cargo check -p tau-memory --target-dir target-fast` pass.
- C-04 (AC-3, integration): targeted session/memory tests pass.

## Success Metrics

- Subtask `#2171` merges with bounded docs + guard updates.
- Wave-9 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
