# Spec #2163

Status: Implemented
Milestone: specs/milestones/m35/index.md
Issue: https://github.com/njfio/Tau/issues/2163

## Problem Statement

Wave-8 `tau-gateway` split helper modules still expose public APIs without
rustdoc markers, and the split-module rustdoc guard does not enforce marker
presence for this module set.

## Acceptance Criteria

- AC-1: Add `///` rustdoc comments for key public APIs in:
  - `crates/tau-gateway/src/gateway_openresponses/openai_compat.rs`
  - `crates/tau-gateway/src/gateway_openresponses/request_translation.rs`
  - `crates/tau-gateway/src/gateway_openresponses/types.rs`
  - `crates/tau-gateway/src/gateway_openresponses/dashboard_status.rs`
- AC-2: Extend `scripts/dev/test-split-module-rustdoc.sh` with wave-8 marker
  assertions for these files.
- AC-3: Scoped compile/test matrix passes for affected crate.

## Scope

In:

- rustdoc additions for wave-8 gateway split helper modules listed above
- guard script assertion expansion for wave-8 markers
- scoped compile/test verification for `tau-gateway`

Out:

- broader documentation rewrites outside scoped files
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): all four wave-8 files contain expected rustdoc
  marker phrases for key public APIs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh`
  fails with missing markers and passes after docs are added.
- C-03 (AC-3, functional):
  `cargo check -p tau-gateway --target-dir target-fast` passes.
- C-04 (AC-3, integration): targeted `tau-gateway` tests covering request
  translation, OpenAI compatibility translation, and dashboard actions pass.

## Success Metrics

- Subtask `#2163` merges with bounded docs + guard updates.
- Wave-8 files no longer appear in zero-doc helper list.
- Conformance suite C-01..C-04 passes.
