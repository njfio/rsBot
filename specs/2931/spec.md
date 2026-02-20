# Spec: Issue #2931 - Implement and validate cortex LLM readiness contracts

Status: Reviewed

## Problem Statement
Current `/cortex/status` reporting provides observer counters but does not expose a deterministic readiness contract (health state, reason codes, failure mode classification) and lacks a first-class live validation command for operators.

## Scope
In scope:
- Add deterministic readiness fields to `/cortex/status`.
- Classify cortex readiness failure modes from observer event artifacts.
- Add an operator live-validation command/procedure for cortex readiness.
- Add/update tests validating readiness classification and endpoint payload contracts.

Out of scope:
- New cortex model-routing behavior.
- Changes to auth modes or endpoint paths.

## Acceptance Criteria
### AC-1 Cortex status exposes readiness health contract
Given authenticated access to `/cortex/status`,
when status is returned,
then payload includes deterministic readiness fields (`health_state`, `rollout_gate`, `reason_code`, `health_reason`) plus supporting readiness signals.

### AC-2 Failure modes are explicit and testable
Given missing/empty/malformed/stale observer artifacts,
when `/cortex/status` is requested,
then reason codes and health state classify each failure mode deterministically.

### AC-3 Live readiness validation procedure is shipped
Given an operator with gateway credentials,
when the live validation command is run,
then it performs an authenticated cortex probe and fails closed when readiness expectations are not met.

### AC-4 Regression and quality gates stay green
Given readiness contract changes,
when scoped gateway tests and quality gates run,
then existing cortex/gateway behavior remains green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | authenticated request | GET `/cortex/status` | readiness fields are present with stable schema semantics |
| C-02 | AC-2 | Regression | missing/empty/malformed/stale artifacts | GET `/cortex/status` | deterministic health/reason classification is returned |
| C-03 | AC-3 | Integration | running gateway + auth credentials | run live readiness command | probe + status check pass/fail deterministically |
| C-04 | AC-4 | Regression | updated gateway module | run scoped tests + fmt/clippy | no regressions/warnings |

## Success Metrics / Signals
- `/cortex/status` includes readiness fields and non-empty reason code semantics.
- New/updated cortex status tests cover missing/empty/malformed/stale classification paths.
- Live readiness command exists and validates both `/cortex/chat` and `/cortex/status` contracts.
- `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and scoped gateway tests pass.
