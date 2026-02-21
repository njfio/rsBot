# Spec: Issue #3152 - Correct Review #35 unresolved claims and add property rate-limit invariants

Status: Reviewed

## Problem Statement
`tasks/review-35.md` reports unresolved gaps that drifted from current `master` state. Specifically, Cortex chat LLM wiring, provider outbound token-bucket rate limiting, and OpenTelemetry export are already implemented. The still-valid risk is thin property-based coverage for policy/rate-limit invariants.

## Scope
In scope:
- Correct `tasks/review-35.md` unresolved tracker rows for:
  - Cortex LLM wiring
  - Provider rate limiting
  - OpenTelemetry export
- Add deterministic conformance script assertions for Review #35 unresolved tracker entries.
- Add property-based tests in `tau-tools` for rate-limit invariants using existing `proptest` support.

Out of scope:
- New Cortex automation behavior.
- New provider throttling architecture.
- New OpenTelemetry exporter behavior.
- New dependencies.

## Acceptance Criteria
### AC-1 Review #35 unresolved tracker reflects current implementation state
Given `tasks/review-35.md`,
when unresolved rows are reviewed,
then Cortex LLM wiring, provider rate limiting, and OpenTelemetry are marked as implemented with concrete evidence references.

### AC-2 Review #35 corrections are guarded by conformance script checks
Given corrected unresolved tracker rows,
when `scripts/dev/test-review-35.sh` runs,
then the script fails on stale claims and passes on corrected state markers.

### AC-3 Tool rate-limit property invariants are covered with randomized tests
Given `ToolPolicy::evaluate_rate_limit` with randomized limits/windows and request schedules,
when rate-limit outcomes are evaluated,
then invariant checks hold:
- allowed calls do not exceed configured capacity per principal/window,
- throttle counters match overflow cardinality,
- `retry_after_ms` is bounded within `[0, window_ms]`,
- principal isolation remains intact.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | Review #35 unresolved table | corrected | stale unresolved claims replaced with implemented evidence |
| C-02 | AC-2 | Conformance | corrected Review #35 file | run `scripts/dev/test-review-35.sh` | script passes and enforces corrected markers |
| C-03 | AC-3 | Property | randomized `max_requests/window/request_count` inputs | evaluate rate limits in-window | allowed count bounded by `max_requests`; throttles match overflow |
| C-04 | AC-3 | Property | randomized `window_ms` and simulated timestamps | evaluate throttled results | `retry_after_ms <= window_ms` always holds |
| C-05 | AC-3 | Property | two principals sharing one policy and same schedule | evaluate each principal | each principal receives independent quota before throttling |

## Success Metrics / Observable Signals
- `cargo test -p tau-tools spec_3152 -- --test-threads=1`
- `scripts/dev/test-review-35.sh`
- `cargo fmt --check`
- `cargo clippy -p tau-tools -- -D warnings`
