# Issue 1676 Spec

Status: Implemented

Issue: `#1676`  
Milestone: `#24`  
Parent: `#1663`

## Problem Statement

M24 requires operator-facing RL lifecycle controls (status/pause/resume/cancel/
rollback) that are policy-gated, idempotent, and auditable. Current runtime
wires prompt-optimization execution but lacks dedicated control-plane command
entrypoints.

## Scope

In scope:

- add CLI flags for RL/prompt-optimization lifecycle control commands
- route lifecycle control command mode through startup preflight dispatch
- enforce RBAC policy checks per lifecycle action via `tau-access`
- persist deterministic control-state and JSONL audit artifacts
- implement status rendering and action idempotency behavior
- add command integration/regression tests

Out of scope:

- distributed live pause/resume signaling across long-running workers (`#1710`)
- crash-recovery orchestration (`#1677`)
- new RBAC schema or multi-tenant auth providers

## Acceptance Criteria

AC-1 (command surface):
Given operator CLI invocations,
when one lifecycle control command is selected,
then startup preflight handles the command and exits without entering prompt
runtime execution.

AC-2 (policy checks):
Given principals and RBAC policy,
when lifecycle actions are requested,
then unauthorized requests fail closed and authorized requests proceed.

AC-3 (idempotent/auditable actions):
Given repeated lifecycle actions,
when commands apply state transitions,
then state updates are idempotent and each request is recorded in an audit log.

AC-4 (rollback command behavior):
Given rollback requests with checkpoint paths,
when rollback command executes,
then checkpoint payload validity is verified and control state records the
rollback target deterministically.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given `--prompt-optimization-control-status`, when startup preflight executes, then command mode is handled and control status output is produced without local prompt runtime dispatch. |
| C-02 | AC-2 | Regression | Given a policy that only allows `control:rl:status`, when principal requests `pause`, then command fails with unauthorized lifecycle diagnostics. |
| C-03 | AC-3 | Functional | Given repeated `pause` requests, when command executes twice, then second transition is marked idempotent and control/audit artifacts remain consistent. |
| C-04 | AC-4 | Functional | Given `--prompt-optimization-control-rollback <checkpoint>`, when checkpoint is valid, then rollback target is stored in control state and audit record includes rollback action metadata. |
| C-05 | AC-4 | Regression | Given rollback path to an invalid checkpoint payload, when command executes, then command fails closed with actionable checkpoint validation error. |

## Success Metrics

- lifecycle control commands are available and preflight-dispatched
- RBAC-denied control actions are blocked with actionable errors
- control-state and audit logs provide deterministic operator evidence
