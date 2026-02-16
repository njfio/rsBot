# Spec #2155

Status: Implemented
Milestone: specs/milestones/m34/index.md
Issue: https://github.com/njfio/Tau/issues/2155

## Problem Statement

Several onboarding/tool-policy split helper modules still expose public APIs
without rustdoc markers, and the split-module guard script does not assert
marker presence for these files.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for key public APIs in:
  - `crates/tau-onboarding/src/startup_daemon_preflight.rs`
  - `crates/tau-onboarding/src/startup_resolution.rs`
  - `crates/tau-tools/src/tool_policy_config.rs`
  - `crates/tau-tools/src/tools/runtime_helpers.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-7 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crates.

## Scope

In:

- rustdoc additions for wave-7 files above
- guard script assertion expansion for wave-7 files
- scoped compile/test verification for `tau-onboarding` and `tau-tools`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): all four wave-7 files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-onboarding --target-dir target-fast` and
  `cargo check -p tau-tools --target-dir target-fast` pass.
- C-04 (AC-3, integration): targeted onboarding/tool-policy tests pass.

## Success Metrics

- Subtask `#2155` merges with bounded docs + guard updates.
- Wave-7 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
