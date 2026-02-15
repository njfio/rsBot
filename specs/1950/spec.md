# Issue 1950 Spec

Status: Implemented

Issue: `#1950`  
Milestone: `#24`  
Parent: `#1658`

## Problem Statement

RL core payload types (`EpisodeTrajectory`, `AdvantageBatch`,
`CheckpointRecord`) currently validate independently. There is no shared
conformance gate that validates cross-payload consistency, which can allow
misaligned ids/lengths/step progression to pass until later stages.

## Scope

In scope:

- add a cross-payload bundle type in `tau-training-types`
- validate intra-bundle alignment between trajectory, advantages, and checkpoint
- provide deterministic, field-oriented bundle validation errors
- add tests for valid and invalid bundle scenarios

Out of scope:

- schema-version changes to existing payload structs
- trainer/runtime orchestration changes
- storage backend migration changes

## Acceptance Criteria

AC-1 (bundle validation API):
Given trajectory, advantage batch, and checkpoint payloads,
when bundle validation runs,
then individual schema validation and cross-payload checks run together.

AC-2 (id/length conformance checks):
Given mismatched trajectory ids or step/advantage counts,
when bundle validation runs,
then it fails with deterministic mismatch errors.

AC-3 (checkpoint progression check):
Given checkpoint global step lower than trajectory step count,
when bundle validation runs,
then it fails closed with deterministic checkpoint progression error.

AC-4 (valid bundle pass):
Given aligned payloads,
when bundle validation runs,
then it returns success with no false positives.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given syntactically valid payloads, when bundle validation runs, then component validators and cross-checks execute together and return success. |
| C-02 | AC-2 | Regression | Given mismatched `trajectory_id` or step/advantage lengths, when bundle validation runs, then deterministic mismatch errors identify both fields. |
| C-03 | AC-3 | Regression | Given `checkpoint.global_step < trajectory.steps.len()`, when bundle validation runs, then deterministic progression error is returned. |
| C-04 | AC-4 | Unit | Given aligned payloads, when bundle validation runs, then it succeeds. |

## Success Metrics

- cross-payload RL conformance has one deterministic validator entrypoint
- id/length/progression mismatches fail before training-time consumption
- tests lock deterministic failure reasons
