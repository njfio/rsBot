# Issue 1630 Spec

Status: Implemented

Issue: `#1630`  
Milestone: `#21`  
Parent: `#1611`

## Problem Statement

M21 marks contract-runner remnants as remove-only, but issue `#1630` remains open without an explicit conformance gate proving removed contract-runner entrypoints stay out of runtime dispatch paths, demos, and operator docs.

## Scope

In scope:

- codify dead contract-runner remnant invariants in a deterministic audit harness
- enforce that removed contract-runner flags are not wired into active startup dispatch paths
- enforce that demo scripts do not invoke removed contract-runner flags
- align transport guide docs with an explicit removed-runner migration matrix

Out of scope:

- removing active contract-runner modes that remain supported (`multi-channel`, `multi-agent`, `gateway`, `deployment`, `voice`)
- changing runtime protocol/wire formats
- adding dependencies

## Acceptance Criteria

AC-1 (inventory/removal posture):
Given M21 scaffold inventory artifacts,
when audited,
then candidate `tau-contract-runner-remnants` remains `remove` with zero runtime/test hits.

AC-2 (dispatch wiring purge):
Given startup dispatch implementation files,
when audited,
then removed contract-runner flags (`memory`, `dashboard`, `browser-automation`, `custom-command`) are not present in non-test dispatch code.

AC-3 (demo/docs alignment):
Given demo scripts and transport guide docs,
when audited,
then demo scripts contain no removed contract-runner flags and docs contain explicit removed-runner migration guidance for all four flags.

AC-4 (scoped verification):
Given conformance harness plus targeted CLI validation tests,
when executed,
then all pass and supported runtime paths remain intact.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `tasks/reports/m21-scaffold-inventory.json`, when audited, then `tau-contract-runner-remnants` is `remove`, `crate_exists=false`, and runtime/test hit counters are zero. |
| C-02 | AC-2 | Regression | Given startup dispatch implementation files, when audited (excluding test blocks), then removed contract-runner fields are absent. |
| C-03 | AC-3 | Functional | Given `docs/guides/transports.md` and `scripts/demo/*.sh`, when audited, then docs include explicit removed-runner migration matrix and demos avoid removed flags. |
| C-04 | AC-4 | Integration | Given targeted tau-onboarding/tau-coding-agent removed-runner CLI tests, when run, then they pass. |

## Success Metrics

- dead contract-runner remnant checks are reproducible via one deterministic harness
- removed entrypoint guidance remains explicit while supported runtime paths remain unchanged
