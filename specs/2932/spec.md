# Spec: Issue #2932 - Close operator documentation and deployment-readiness gaps

Status: Reviewed

## Problem Statement
Operator docs are spread across multiple runbooks without one deterministic P0 readiness procedure that combines gateway health, cortex readiness, control-plane status, deployment posture, and rollback actions. This creates operational drift risk and slows go/no-go decisions.

## Scope
In scope:
- Add a consolidated operator readiness runbook for gateway/cortex/control-plane/deployment checks.
- Add a deterministic live validation script for operator readiness checks with fail-closed behavior.
- Cross-link and update existing runbooks/docs index to route operators to the canonical readiness procedure.
- Add script-level tests for success and fail-closed readiness behavior.

Out of scope:
- Runtime behavior changes for gateway/cortex/deployment internals.
- New auth modes, endpoint shape changes, or provider routing changes.

## Acceptance Criteria
### AC-1 Canonical readiness runbook exists
Given an operator preparing promotion,
when they follow the runbook,
then they can run a deterministic checklist covering gateway status, cortex readiness, operator summary, deployment status, and rollback posture.

### AC-2 Readiness validator is fail-closed
Given a running gateway/control-plane,
when the live readiness script is executed,
then it exits non-zero on missing required fields, unexpected health state, or `rollout_gate=hold`.

### AC-3 Docs are integrated and discoverable
Given `docs/README.md` and existing ops runbooks,
when an operator navigates docs,
then the canonical readiness runbook and validator command are linked from relevant entrypoints.

### AC-4 Regression checks pass
Given documentation/script updates,
when script tests and runbook ownership doc checks run,
then they pass with no regressions.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | operator with repo access | read canonical readiness runbook | runbook includes checklist, gate interpretation, and rollback actions |
| C-02 | AC-2 | Conformance | healthy mocked gateway/cortex/control output | run readiness validator | script returns pass and reports expected gate/health values |
| C-03 | AC-2 | Regression | mocked hold/degraded output | run readiness validator | script fails closed with actionable error |
| C-04 | AC-3 | Functional | docs entrypoints | inspect docs index + runbooks | canonical runbook/validator references are present |
| C-05 | AC-4 | Regression | updated docs/scripts | run script tests + runbook ownership check | all validations pass |

## Success Metrics / Signals
- One canonical runbook path for P0 readiness is published and linked.
- `scripts/dev/operator-readiness-live-check.sh` exists with deterministic pass/fail semantics.
- Script test harness proves both healthy and fail-closed cases.
- Documentation index and related runbooks point to the canonical procedure.
