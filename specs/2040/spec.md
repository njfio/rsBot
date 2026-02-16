# Spec #2040

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2040

## Problem Statement

`crates/tau-cli/src/cli_args.rs` exceeded the decomposition budget and slowed
maintainability/review velocity. M25 requires reducing the primary file below
3000 LOC while preserving CLI behavior.

## Acceptance Criteria

- AC-1: Primary file is reduced below 3000 LOC.
- AC-2: CLI validation and integration checks remain green after split.
- AC-3: Split map, ownership, and migration evidence are documented through
  child issues.

## Scope

In:

- Define split map and ownership (`#2058`).
- Execute code extraction and parity checks (`#2059`).

Out:

- Decomposition of other oversized files under Story `#2032`.

## Conformance Cases

- C-01 (AC-1): `wc -l crates/tau-cli/src/cli_args.rs` reports `<3000`.
- C-02 (AC-2): `scripts/dev/test-cli-args-domain-split.sh` and scoped Rust
  checks/tests pass after extraction.
- C-03 (AC-3): split map artifacts exist in `tasks/reports/` and are covered by
  contract tests.

## Success Metrics

- `cli_args.rs` remains below threshold with reproducible split-map and parity
  evidence.
