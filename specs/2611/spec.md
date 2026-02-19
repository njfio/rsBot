# Spec: Issue #2611 - Outbound provider token-bucket rate limiting

Status: Implemented

## Problem Statement
Provider retries and bursty prompt traffic can create outbound request spikes that exceed upstream quotas and trigger avoidable 429 cascades. Tau needs configurable, deterministic outbound provider throttling that is fail-closed when wait budgets are exhausted.

## Acceptance Criteria

### AC-1 Provider rate-limit configuration is explicit and deterministic
Given CLI defaults and overrides,
When provider rate-limit flags are parsed,
Then capacity/refill/wait-budget values are populated deterministically and disabled by default.

### AC-2 Outbound provider requests are token-bucket gated
Given a configured token bucket with limited capacity,
When burst requests exceed immediate token availability,
Then calls are delayed up to configured wait budget and fail closed when budget is exceeded.

### AC-3 Provider throttling wrapper preserves successful behavior for allowed calls
Given outbound rate limiting is enabled and a call is allowed by the token bucket,
When the wrapped provider client executes a completion,
Then the response semantics match the inner client behavior.

### AC-4 Scoped verification gates are green
Given the new provider throttling logic/tests,
When scoped checks run,
Then format, lint, and crate tests pass.

## Scope

### In Scope
- CLI flags for provider outbound token-bucket configuration.
- `tau-provider` wrapper around `LlmClient` enforcing token-bucket gating.
- AC-mapped unit/regression/integration tests for limiter behavior and client integration.

### Out of Scope
- Per-model adaptive quota discovery from provider APIs.
- Distributed/shared throttling across multiple Tau processes.
- Gateway transport rate limits (already covered by gateway auth runtime).

## Conformance Cases
- C-01 (unit): CLI default/override parsing for provider outbound limiter flags.
- C-02 (functional): limiter delays burst calls when capacity is exhausted but wait budget permits.
- C-03 (regression): limiter fails closed when required wait exceeds configured max wait.
- C-04 (integration): `ProviderRateLimitedClient` returns successful responses for allowed calls while enforcing limiter gates.
- C-05 (verify): scoped `fmt`, `clippy -p tau-provider`, `test -p tau-provider`, and `test -p tau-coding-agent cli_provider_rate_limit_flags` pass.

## Success Metrics / Observable Signals
- New provider limiter tests pass consistently.
- No regressions in existing provider client/fallback tests.
- Outbound limiter can be disabled by default and enabled explicitly with deterministic behavior.
