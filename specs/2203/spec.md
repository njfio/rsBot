# Spec #2203

Status: Implemented
Milestone: specs/milestones/m40/index.md
Issue: https://github.com/njfio/Tau/issues/2203

## Problem Statement

The current `allow(...)` inventory under `crates/` still contains a stale
`#[allow(dead_code)]` in PPO tests, and the audit documentation no longer
reflects the true current suppression set.

## Acceptance Criteria

- AC-1: Remove stale `#[allow(dead_code)]` suppression in
  `crates/tau-algorithm/src/ppo.rs` without changing runtime behavior.
- AC-2: Publish wave-2 audit documentation with current `allow(...)` inventory,
  removals completed, and retained rationale.
- AC-3: Scoped compile/test/lint checks pass for affected crate(s).

## Scope

In:

- `tau-algorithm` stale suppression cleanup in PPO test module
- wave-2 audit documentation update under `docs/guides/`
- scoped `tau-algorithm` verification

Out:

- elimination of all remaining suppressions in this wave
- unrelated algorithm/runtime behavior changes

## Conformance Cases

- C-01 (AC-1, regression):
  `rg -n "#\\[allow\\(dead_code\\)\\]" crates/tau-algorithm/src/ppo.rs` returns no matches after change.
- C-02 (AC-2, functional):
  `docs/guides/allow-pragmas-audit-wave2.md` exists and lists current inventory with retained rationale.
- C-03 (AC-3, functional):
  `cargo check -p tau-algorithm --target-dir target-fast` passes.
- C-04 (AC-3, integration):
  `cargo test -p tau-algorithm ppo --target-dir target-fast` passes.
- C-05 (AC-3, regression):
  `cargo clippy -p tau-algorithm --target-dir target-fast -- -D warnings` passes.

## Success Metrics

- Subtask `#2203` merges with one stale suppression removed.
- Wave-2 audit guide reflects the real remaining suppression set.
- Conformance suite C-01..C-05 passes.
