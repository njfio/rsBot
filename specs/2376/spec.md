# Spec: Issue #2376 - Enforce Cumulative Session Usage/Cost Tracking Across Runtimes

Status: Accepted

## Problem Statement
Usage and estimated-cost deltas are emitted in multiple runtime paths (CLI runtime loop and gateway OpenResponses). This slice requires deterministic proof that per-session totals are cumulatively persisted and exposed after reload, so session-level cost tracking is reliable in production.

## Acceptance Criteria

### AC-1 Session store cumulatively persists usage and cost deltas
Given a session store receives multiple `record_usage_delta` calls,
When the store is reloaded,
Then `input_tokens`, `output_tokens`, `total_tokens`, and `estimated_cost_usd` equal cumulative sums.

### AC-2 CLI runtime loop accumulates usage/cost across consecutive prompts in one session
Given `run_prompt_with_cancellation` is executed twice against one `SessionRuntime`,
When prompt responses include token usage,
Then session usage/cost totals accumulate across both prompts.

### AC-3 Gateway OpenResponses accumulates usage/cost across requests in one session
Given two `/v1/responses` requests with the same `session_id`,
When both requests complete,
Then persisted session usage includes cumulative tokens and non-decreasing cumulative estimated cost.

### AC-4 Session stats exposes accumulated usage totals
Given a session with accumulated usage summary,
When session stats are rendered,
Then text and JSON outputs expose the same cumulative usage/cost values.

## Scope

### In Scope
- Conformance coverage for cumulative session usage/cost persistence and visibility.
- Runtime call-site validation for `tau-coding-agent` and `tau-gateway`.

### Out of Scope
- Model catalog cost constant updates.
- Multi-session aggregation/reporting.
- Provider invoice reconciliation.

## Conformance Cases

| Case | AC | Tier | Input | Expected |
|---|---|---|---|---|
| C-01 | AC-1 | Integration | Two explicit `record_usage_delta` calls then reload | Reloaded usage equals cumulative token/cost totals |
| C-02 | AC-2 | Functional/Integration | Two sequential runtime prompts with usage in one `SessionRuntime` | Session usage summary totals equal sum of both prompt deltas |
| C-03 | AC-3 | Integration | Two gateway `/v1/responses` requests with same session id | Persisted usage tokens equal sum; estimated cost is non-decreasing and persisted |
| C-04 | AC-4 | Functional | Session stats text/json render after usage accumulation | Both formats expose same cumulative usage/cost numbers |

## Success Metrics / Observable Signals
- All C-01..C-04 tests pass.
- No regression in existing session/gateway runtime suites.
- Scoped mutation test for touched diff has zero missed mutants.
