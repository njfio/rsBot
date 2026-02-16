# Spec #2115

Status: Implemented
Milestone: specs/milestones/m29/index.md
Issue: https://github.com/njfio/Tau/issues/2115

## Problem Statement

Story M29.1 tracks second-wave split-module rustdoc coverage with guardrail
expansion. Task/subtask delivery (`#2114/#2113`) merged via PRs `#2117/#2116`.
This story closes by consolidating AC/conformance evidence.

## Acceptance Criteria

- AC-1: second-wave split helper modules are documented.
- AC-2: split-module rustdoc guard covers second-wave scope.
- AC-3: story-level lifecycle artifacts map ACs to test evidence.

## Scope

In:

- consume merged outputs from `#2114/#2113`
- publish story-level lifecycle artifacts
- rerun scoped verification commands

Out:

- new documentation waves beyond M29.1
- non-documentation behavior changes

## Conformance Cases

- C-01 (AC-1, functional): second-wave files contain expected rustdoc markers.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh` passes.
- C-03 (AC-2, functional): compile checks pass for touched crates.
- C-04 (AC-3, integration): targeted tests pass for touched modules.

## Success Metrics

- Story issue `#2115` closes with linked task/subtask evidence.
- `specs/2115/{spec,plan,tasks}.md` lifecycle is complete.
- Epic `#2111` roll-up is unblocked.
