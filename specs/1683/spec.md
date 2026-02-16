# Issue 1683 Spec

Status: Accepted

Issue: `#1683`  
Milestone: `#21`  
Parent: `#1636`

## Problem Statement

`crates/tau-ops/src/channel_store_admin.rs` has already been decomposed into
helper modules, but this subtask remains open without issue-scoped spec
artifacts and explicit conformance evidence for command/report/repair
boundaries.

## Scope

In scope:

- add `specs/1683/{spec,plan,tasks}.md`
- add split-conformance harness for channel-store-admin domains
- verify root file line budget and module boundaries
- run targeted tests and quality checks

Out of scope:

- changing operator command semantics
- changing output schemas/CLI contracts
- adding new channel-store admin features

## Acceptance Criteria

AC-1 (line budget):
Given `crates/tau-ops/src/channel_store_admin.rs`,
when conformance checks run,
then line count is below `3000`.

AC-2 (domain extraction):
Given command/report/repair support domains,
when conformance checks run,
then module declarations and extracted files are present under
`crates/tau-ops/src/channel_store_admin/`.

AC-3 (behavior parity):
Given existing channel-store-admin tests,
when targeted tests run,
then behavior remains green.

AC-4 (verification):
Given issue-scope checks,
when run,
then harness + targeted tests + roadmap/fmt/clippy pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given split root file, when harness runs, then `channel_store_admin.rs < 3000` lines. |
| C-02 | AC-2 | Functional | Given module tree, when harness runs, then required module markers/files are present. |
| C-03 | AC-3 | Regression | Given targeted tests, when run, then channel-store-admin behavior remains green. |
| C-04 | AC-4 | Integration | Given issue commands, when run, then harness + tests + roadmap/fmt/clippy are green. |

## Success Metrics

- root file remains below threshold with split boundaries intact
- extracted module domains are verifiable via harness
- issue closure includes explicit spec-driven conformance evidence
