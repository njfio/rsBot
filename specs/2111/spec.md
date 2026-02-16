# Spec #2111

Status: Implemented
Milestone: specs/milestones/m29/index.md
Issue: https://github.com/njfio/Tau/issues/2111

## Problem Statement

Epic M29 governs second-wave split-module rustdoc baseline expansion and
regression guardrail strengthening. Story/task/subtask chain
(`#2115/#2114/#2113`) merged via PRs `#2118/#2117/#2116`; this epic closes by
consolidating AC/conformance evidence.

## Acceptance Criteria

- AC-1: M29.1 hierarchy (`#2115/#2114/#2113`) is merged and closed.
- AC-2: second-wave doc guard and compile/test compatibility checks pass.
- AC-3: epic-level lifecycle artifacts map ACs to conformance evidence.

## Scope

In:

- consume merged M29.1 story/task/subtask outputs
- publish epic-level lifecycle artifacts and evidence mapping
- rerun scoped verification commands on latest `master`

Out:

- additional waves beyond M29 second-wave scope
- runtime feature behavior changes unrelated to docs baseline

## Conformance Cases

- C-01 (AC-1, governance): issues `#2115/#2114/#2113` are closed with merged PRs.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh` passes.
- C-03 (AC-2, functional): compile checks pass for touched crates.
- C-04 (AC-2, integration): targeted tests pass for touched modules.

## Success Metrics

- Epic `#2111` closes with linked conformance evidence.
- `specs/2111/{spec,plan,tasks}.md` lifecycle is complete.
- Milestone M29 can close when no open scoped issues remain.
