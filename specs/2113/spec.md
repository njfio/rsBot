# Spec #2113

Status: Implemented
Milestone: specs/milestones/m29/index.md
Issue: https://github.com/njfio/Tau/issues/2113

## Problem Statement

A second wave of split helper modules still exposes public APIs without
rustdoc comments, and the existing guard script does not cover these files.
We need bounded rustdoc additions plus guardrail expansion to prevent regression.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for public helper APIs in:
  - `crates/tau-github-issues/src/github_transport_helpers.rs`
  - `crates/tau-github-issues/src/issue_filter.rs`
  - `crates/tau-events/src/events_cli_commands.rs`
  - `crates/tau-deployment/src/deployment_wasm_runtime.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with second-wave
  marker assertions covering these files.
- AC-3: Scoped compile/test matrix passes for affected crates.

## Scope

In:

- rustdoc additions for second-wave public helper APIs
- guard script assertion expansion for second-wave files
- scoped compile/test verification for touched crates

Out:

- broad documentation rewrites outside scoped files
- non-documentation behavioral changes

## Conformance Cases

- C-01 (AC-1, functional): all four scoped files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails when required markers are missing and passes once present.
- C-03 (AC-3, functional):
  `cargo check -p tau-github-issues --target-dir target-fast`,
  `cargo check -p tau-events --target-dir target-fast`,
  `cargo check -p tau-deployment --target-dir target-fast` pass.
- C-04 (AC-3, integration): targeted tests for touched modules pass.

## Success Metrics

- Subtask `#2113` merges with bounded doc+guard updates.
- Second-wave files are no longer zero-doc helper modules.
- Conformance suite C-01..C-04 passes.
