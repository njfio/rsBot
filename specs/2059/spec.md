# Spec #2059

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2059

## Problem Statement

`crates/tau-cli/src/cli_args.rs` exceeded the M25 maintainability threshold and
required an execution split that preserves clap/runtime behavior while reducing
the primary file below 3000 LOC.

## Acceptance Criteria

- AC-1: `cli_args.rs` is reduced below 3000 LOC through module extraction.
- AC-2: Existing split guardrail tests pass with updated threshold and module
  markers.
- AC-3: Unit/functional/integration/regression evidence is captured for the
  split execution wave.

## Scope

In:

- Execute module extraction and flatten wiring for CLI argument domains.
- Update split guardrail tests to enforce `<3000` threshold.
- Run and record scoped validation commands.

Out:

- Renaming CLI flags or changing runtime semantics.
- Decomposing unrelated oversized files (`tools.rs`, `github_issues_runtime.rs`,
  `channel_store_admin.rs`).

## Conformance Cases

- C-01 (AC-1, functional): `wc -l crates/tau-cli/src/cli_args.rs` reports
  `<3000`.
- C-02 (AC-2, regression): `scripts/dev/test-cli-args-domain-split.sh` enforces
  `<3000` and validates new extraction module markers.
- C-03 (AC-3, integration): compile/test evidence captured from scoped Rust and
  governance test commands.

## Success Metrics

- Primary CLI args file remains below target threshold with parity markers
  validated by guardrail tests.
