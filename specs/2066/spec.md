# Spec #2066

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2066

## Problem Statement

`crates/tau-ops/src/channel_store_admin.rs` is above the M25.3.5 target and
lacks a deterministic split-map contract that defines phased extraction
boundaries, ownership, import/API impact, and migration checks before
implementation changes.

## Acceptance Criteria

- AC-1: Deterministic split-map artifacts are generated for
  `channel_store_admin.rs` with phased extraction map and line-budget context.
- AC-2: Public API and import impact are documented for the planned split.
- AC-3: Test migration plan and fail-closed validation checks exist before
  code moves.

## Scope

In:

- Add split-map generator, schema, report artifacts, guide, and contract tests
  for M25.3.5.
- Capture current-line baseline and target-line gap (<2200 target).

Out:

- Code extraction itself (implemented in `#2067`).

## Conformance Cases

- C-01 (AC-1, functional): generator emits deterministic JSON + Markdown
  artifacts including line budget, current lines, and phased extraction plan.
- C-02 (AC-2, integration): guide/report include non-empty public API and
  import impact sections.
- C-03 (AC-3, regression): split-map tests fail closed on invalid source/target
  inputs and enforce required artifact presence.

## Success Metrics

- Maintainers have executable, tested split-map planning artifacts accepted
  before channel-store admin decomposition begins.
