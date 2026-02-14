# Provider Fallback Circuit Breaker

## Purpose
Add circuit-breaker behavior to provider fallback routing so unhealthy routes are temporarily skipped after repeated retryable failures.

## Scope
- Implemented in `crates/tau-provider/src/fallback.rs`
- Applied to `FallbackRoutingClient` failover flow
- Enabled by default with production-safe limits

## Default Behavior
- `enabled = true`
- `failure_threshold = 3`
- `cooldown_ms = 30000`

For each route:
- Retryable failures increment a consecutive-failure counter.
- When counter reaches threshold, the route is marked open until `now + cooldown_ms`.
- Open routes are skipped until cooldown expires.
- Successful route responses reset that route's failure state.

## Retryable Error Classes
Circuit-breaker accounting uses the same retryability rules as fallback handoff:
- HTTP status: `408`, `409`, `425`, `429`, `>=500`
- transport-level retryable HTTP errors (timeout/connect/request/body)

Non-retryable failures do not open the circuit.

## Observability Events
When `event_sink` is configured, fallback emits:
- `provider_fallback`
- `provider_circuit_opened`
- `provider_circuit_skip`

These events include route/model identity and routing metadata to support debugging and incident analysis.

## Validation Coverage
Added/updated in `crates/tau-provider/src/fallback.rs`:
- Unit:
  - default circuit-breaker config values
  - retryable error classification
- Functional:
  - circuit opens and skips unhealthy route after threshold failures
- Integration:
  - route is retried after cooldown expiration and can recover
  - streaming fallback behavior remains intact
- Regression:
  - non-retryable errors do not trip circuit
  - all-open routes fail fast with deterministic error behavior
