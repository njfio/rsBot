# Spec #2438

Status: Implemented
Milestone: specs/milestones/m74/index.md
Issue: https://github.com/njfio/Tau/issues/2438

## Problem Statement

PR #2435 is failing due to stale roadmap status docs, and contributors lack a
single local preflight command that mirrors high-signal CI blockers. This slows
iteration and causes avoidable CI churn.

## Scope

In scope:

- Sync roadmap status docs consumed by `roadmap-status-sync --check`.
- Add `scripts/dev/preflight-fast.sh` wrapper to run:
  - `scripts/dev/roadmap-status-sync.sh --check --quiet`
  - `scripts/dev/fast-validate.sh` (with passthrough args)
- Add test script coverage for wrapper behavior.

Out of scope:

- Changes to CI YAML workflow composition.
- Non-shell runtime changes.

## Acceptance Criteria

- AC-1: Given current branch state, when running
  `scripts/dev/roadmap-status-sync.sh --check --quiet`, then check passes.
- AC-2: Given developer usage of fast preflight, when running
  `scripts/dev/preflight-fast.sh <args>`, then it enforces roadmap freshness
  before invoking `fast-validate` with passthrough args.
- AC-3: Given script regressions, when running test coverage for preflight
  wrapper, then failures are surfaced for missing steps or bad passthrough.

## Conformance Cases

- C-01 (AC-1, conformance): roadmap freshness check returns exit 0 on branch.
- C-02 (AC-2, conformance): preflight wrapper forwards arguments unchanged to
  `fast-validate` and exits non-zero if roadmap check fails.
- C-03 (AC-3, conformance): preflight wrapper tests pass with deterministic
  fixture stubs.

## Success Metrics / Observable Signals

- `scripts/dev/roadmap-status-sync.sh --check --quiet` passes locally.
- `scripts/dev/test-preflight-fast.sh` passes.
- PR #2435 CI checks transition to green after push/rerun.
