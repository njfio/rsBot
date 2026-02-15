# Issue 1677 Spec

Status: Implemented

Issue: `#1677`  
Milestone: `#24`  
Parent: `#1663`

## Problem Statement

RL/prompt-optimization jobs need deterministic crash recovery so interrupted
runs can resume with explicit guardrails, replayed control history, and
checkpoint-backed restoration metadata.

## Scope

In scope:

- add crash-detection and recovery flow for lifecycle `resume` control action
- replay control audit history into deterministic recovery metadata
- restore policy checkpoint state using primary/fallback checkpoint paths
- fail closed when recovery prerequisites are missing/corrupt
- persist machine-readable recovery report for operators
- add runbook documentation for recovery actions

Out of scope:

- distributed runtime worker orchestration during resume
- remote checkpoint backends
- training algorithm changes

## Acceptance Criteria

AC-1 (crash detection + replay):
Given interrupted lifecycle/control state,
when resume recovery executes,
then crash detection is deterministic and replay metadata includes audit
history counts.

AC-2 (checkpoint recovery path):
Given primary/fallback checkpoints,
when resume recovery executes,
then recovery loads primary when valid or falls back deterministically with
diagnostics.

AC-3 (resume guardrails):
Given missing/corrupt recovery prerequisites,
when resume recovery executes,
then command fails closed with actionable errors.

AC-4 (operator evidence):
Given recovery execution,
when control command completes,
then recovery report artifact is written and runbook guidance is documented.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given interrupted running state and prior audit rows, when `resume` executes, then recovery report marks crash detected and includes deterministic replay counts. |
| C-02 | AC-2 | Functional | Given valid primary checkpoint, when `resume` executes, then report records primary recovery source and checkpoint metadata. |
| C-03 | AC-2 | Regression | Given corrupted primary and valid fallback checkpoint, when `resume` executes, then fallback source is used and diagnostics capture primary failure. |
| C-04 | AC-3 | Regression | Given crash-detected state with no available checkpoint, when `resume` executes, then command fails with resume guardrail error. |
| C-05 | AC-4 | Functional | Given successful recovery execution, when artifacts are inspected, then `recovery-report.json` schema fields and documented operator runbook steps exist. |

## Success Metrics

- recovery behavior is deterministic and test-backed for primary/fallback flows
- guardrails block unsafe resume conditions
- operators receive actionable recovery metadata and runbook instructions
