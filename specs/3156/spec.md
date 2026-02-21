# Spec: Issue #3156 - Property invariants for rate-limit reset, disable, and payload contracts

Status: Reviewed

## Problem Statement
Wave 1 improved property coverage for core rate-limit capacity/retry/principal isolation. Remaining risk is unverified invariant depth for reset semantics, disabled-limiter semantics, and error payload contract stability in gate-level decisions.

## Scope
In scope:
- Add property tests for `ToolPolicy::evaluate_rate_limit` reset behavior at/after window boundary.
- Add property tests for disabled limiter behavior (`max_requests=0` or `window_ms=0`).
- Add property tests for `evaluate_tool_rate_limit_gate` payload contract fields across reject/defer behavior modes.

Out of scope:
- Runtime behavior changes in rate-limiter implementation.
- Provider-layer limiter changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Window-boundary reset replenishes quota deterministically
Given a principal with exhausted quota in a window,
when evaluation occurs at/after `window_start + window_ms`,
then the principal can consume new quota before throttling recurs.

### AC-2 Disabled limiter configurations never throttle
Given limiter configuration with `max_requests=0` or `window_ms=0`,
when rate-limit evaluation is executed repeatedly,
then evaluation always allows and throttle counters remain zero.

### AC-3 Gate-level throttle payload contract is stable across behaviors
Given rate-limit gate throttling in both reject/defer modes,
when the gate returns a denied `ToolExecutionResult`,
then contract fields (`policy_rule`, `policy_decision`, `decision`, `reason_code`, `principal`, `action`, `payload`, limits, retry metadata) remain consistent and behavior-specific values are correct.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Property | randomized `max_requests/window_ms` | exhaust quota then evaluate at boundary | first post-boundary call is allowed |
| C-02 | AC-1 | Property | randomized `max_requests/window_ms` | evaluate second post-boundary call | throttling recurs only after replenished capacity is consumed |
| C-03 | AC-2 | Property | disabled config branch A (`max_requests=0`) | repeated evaluations | no throttles and zero throttle counters |
| C-04 | AC-2 | Property | disabled config branch B (`window_ms=0`) | repeated evaluations | no throttles and zero throttle counters |
| C-05 | AC-3 | Property | randomized behavior mode + tool/payload/principal | trigger second gate call | denied payload includes stable contract fields and correct mode-specific values |

## Success Metrics / Observable Signals
- `cargo test -p tau-tools spec_3156 -- --test-threads=1`
- `cargo test -p tau-tools spec_3152 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-tools -- -D warnings`
