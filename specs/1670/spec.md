# Issue 1670 Spec

Status: Implemented

Issue: `#1670`  
Milestone: `#24`  
Parent: `#1663`

## Problem Statement

M24 requires resumable RL training. The current training stack has no
checkpoint persistence for policy and optimizer state, and no rollback-aware
resume path when a primary checkpoint is corrupted.

## Scope

In scope:

- add policy checkpoint save/load in `tau-trainer`
- persist policy + optimizer state with versioned checkpoint metadata
- add rollback-aware resume loading with actionable diagnostics
- add conformance tests for roundtrip, corruption fallback, and version errors

Out of scope:

- distributed checkpoint registry or remote blob storage
- model-parameter binary format optimization
- online checkpoint promotion policy decisions (covered by `benchmark_significance`)

## Acceptance Criteria

AC-1 (checkpoint roundtrip):
Given a valid checkpoint payload,
when saved and then loaded,
then policy state, optimizer state, and step counters are preserved exactly.

AC-2 (rollback resume path):
Given a corrupted primary checkpoint and a valid fallback checkpoint,
when resume loading executes,
then loading succeeds from fallback and emits actionable diagnostics about the
primary failure.

AC-3 (versioned restore guard):
Given a checkpoint payload with an unsupported checkpoint version,
when loading executes,
then loading fails with an explicit unsupported-version error.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given a valid checkpoint, when save/load executes, then loaded state exactly matches saved policy/optimizer payload and step metadata. |
| C-02 | AC-2 | Regression | Given corrupted primary + valid fallback checkpoints, when rollback load executes, then source is fallback and diagnostics include primary-load failure details. |
| C-03 | AC-3 | Regression | Given unsupported checkpoint version, when load executes, then it fails with deterministic unsupported-version error text. |

## Success Metrics

- resume path supports policy + optimizer checkpoint continuity
- corrupted primary checkpoints no longer block resume if fallback is available
- version mismatch failures are explicit and actionable
