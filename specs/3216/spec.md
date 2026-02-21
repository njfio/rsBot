# Spec: Issue #3216 - move /gateway/status handler into status_runtime module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still carries a large inline `handle_gateway_status` implementation. Further modularization should isolate this handler while preserving endpoint contract behavior and continuing the downward hotspot trend.

## Scope
In scope:
- Move `handle_gateway_status` to new `gateway_openresponses/status_runtime.rs`.
- Rewire routing/imports to use extracted handler.
- Tighten gateway-openresponses size guard threshold.

Out of scope:
- New status fields or schema changes.
- Auth/rate-limit behavior changes.
- Route additions/removals.

## Acceptance Criteria
### AC-1 status payload contract remains stable after handler extraction
Given existing gateway status integration fixtures,
when running status endpoint tests,
then `service`, `events`, and endpoint metadata assertions remain unchanged.

### AC-2 size guard ratchet tightens and remains green
Given the post-extraction module layout,
when running the gateway size guard,
then `gateway_openresponses.rs` remains under the tightened threshold and `status_runtime.rs` is wired.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Conformance/Integration | status fixture with no events runtime | `integration_gateway_status_endpoint_returns_service_snapshot` | events reason `events_not_configured` + core status fields unchanged |
| C-02 | AC-1 | Conformance/Integration | status fixture with events runtime configured | `integration_gateway_status_endpoint_returns_events_status_when_configured` | events reason `events_ready` + counters unchanged |
| C-03 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold passes and status module wiring exists |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_events_status_when_configured`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
