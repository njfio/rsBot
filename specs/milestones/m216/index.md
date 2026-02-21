# M216 - Property-Test Depth Wave 2 for Tool Policy Invariants

Status: In Progress

## Context
Review #35 identified property-based testing depth as a remaining quality gap. Wave 1 added core rate-limit capacity/retry/principal-isolation invariants. This wave extends coverage to reset behavior, disabled-limiter behavior, and gate payload contract invariants.

## Scope
- Add property invariants for window reset/replenish behavior.
- Add property invariants for disabled limiter behavior (`max_requests=0` or `window_ms=0`).
- Add property invariants for `evaluate_tool_rate_limit_gate` error payload contracts across reject/defer behaviors.

## Linked Issues
- Epic: #3155
- Story: #3154
- Task: #3156

## Success Signals
- `cargo test -p tau-tools spec_3156 -- --test-threads=1` passes.
- Existing `spec_3152` property suite remains green.
