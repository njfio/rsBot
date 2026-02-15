# Issue 1727 Spec

Status: Implemented

Issue: `#1727`  
Milestone: `#24`  
Parent: `#1663`

## Problem Statement

RL lifecycle operations (status/pause/resume/cancel/rollback) need explicit
RBAC authorization hooks so control-plane actions are blocked when principals
lack policy permission.

## Scope

In scope:

- define a stable RL lifecycle action model in `tau-access`
- map lifecycle actions to RBAC action keys under `control:rl:*`
- authorize lifecycle actions through existing RBAC policy evaluation
- enforce denied decisions with actionable error diagnostics
- add negative authorization tests proving blocked control actions

Out of scope:

- adding new CLI flags or runtime transport commands (`#1676`)
- changes to RBAC policy schema
- non-RL control-plane permission surfaces

## Acceptance Criteria

AC-1 (action model + parsing):
Given RL lifecycle action tokens,
when parsed into typed actions,
then supported tokens map deterministically and unsupported tokens fail with an
actionable error.

AC-2 (policy authorization):
Given an RBAC policy and principal,
when authorizing an RL lifecycle action,
then authorization uses `control:rl:<action>` keys and returns allow/deny
decisions consistent with RBAC rules.

AC-3 (enforcement):
Given a denied RL lifecycle authorization result,
when enforcement executes,
then it returns an error that includes principal and action key for operator
triage.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given `status|pause|resume|cancel|rollback` and an invalid token, when parsed, then valid inputs map to enum variants and invalid input fails with usage guidance. |
| C-02 | AC-2 | Functional | Given a policy with `control:rl:*` and `control:rl:status` roles, when authorizing operator/viewer principals, then decisions match policy allow/deny outcomes for lifecycle actions. |
| C-03 | AC-3 | Regression | Given a principal lacking pause permission, when enforcement runs for `pause`, then the error contains `unauthorized rl lifecycle action`, `principal=<id>`, and `action=control:rl:pause`. |
| C-04 | AC-2 | Integration | Given an explicit policy path and stable action-key mapper, when each lifecycle action is converted, then keys remain exactly `control:rl:<action>`. |

## Success Metrics

- all RL lifecycle actions have stable RBAC key mapping with conformance tests
- denied RL lifecycle controls are blocked by enforcement helpers
- operator errors include enough context to debug policy denials quickly
