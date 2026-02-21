# Spec: Issue #3212 - extract gateway events status collector with contract parity

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still contains event-scheduler status report types and collection logic inline, increasing hotspot pressure and making targeted maintenance harder. This logic is separable and should be moved without changing operator-visible status payload behavior.

## Scope
In scope:
- Move events status report structs and collection logic to a focused module.
- Keep `GET /gateway/status` `events` object contract and reason-code behavior unchanged.
- Add deterministic guardrail for gateway hotspot size trend.

Out of scope:
- Any API route additions/removals.
- Scheduler behavior changes.
- Auth/rate-limit behavior changes.

## Acceptance Criteria
### AC-1 events status payload contract remains stable after extraction
Given gateway status fixtures for both unconfigured and configured events states,
when calling `GET /gateway/status`,
then `events.reason_code`, `events.rollout_gate`, and baseline counters remain unchanged.

### AC-2 extraction reduces root hotspot pressure with explicit guard
Given the modularized gateway surface,
when running the gateway size guard,
then `gateway_openresponses.rs` stays under the configured threshold and the extracted module file exists.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Conformance/Integration | events unconfigured fixture | `integration_gateway_status_endpoint_returns_service_snapshot` | `events.reason_code=events_not_configured`, rollout gate pass |
| C-02 | AC-1 | Conformance/Integration | events configured fixture | `integration_gateway_status_endpoint_returns_events_status_when_configured` | `events.reason_code=events_ready`, counters/last reason stable |
| C-03 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | root module line count <= threshold and `events_status.rs` present |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_events_status_when_configured`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
