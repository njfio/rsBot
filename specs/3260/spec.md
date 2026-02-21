# Spec: Issue #3260 - move websocket and stream handlers to dedicated module

Status: Reviewed

## Problem Statement
`gateway_openresponses.rs` still owns WebSocket/session stream handlers (`handle_gateway_ws_upgrade`, `run_dashboard_stream_loop`) that can be isolated without behavior change. Keeping them in root increases module density and weakens ownership boundaries.

## Scope
In scope:
- Move ws/stream helper handlers from root into `gateway_openresponses/ws_stream_handlers.rs`.
- Preserve ws auth/session semantics and dashboard stream output contracts.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Endpoint path or payload contract changes.
- Authentication model changes.
- Event-stream schema changes.

## Acceptance Criteria
### AC-1 websocket and stream contracts remain stable
Given existing ws/stream functional tests,
when tests run,
then ws upgrade auth/token behavior and dashboard stream behavior remain unchanged.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when root guard runs,
then root line count is under tightened threshold and moved ws/stream function definitions are no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | missing session token request | `functional_gateway_ws_upgrade_missing_session_token_returns_unauthorized` | ws upgrade is rejected with unauthorized response |
| C-02 | AC-1 | Functional/Conformance | valid session token request | `functional_gateway_ws_upgrade_includes_session_token_when_present` | upgrade path includes session token and preserves headers |
| C-03 | AC-1 | Functional/Conformance | dashboard stream request | `functional_dashboard_stream_returns_sse_when_requested` | stream endpoint returns SSE contract |
| C-04 | AC-1 | Regression/Conformance | multi-event stream request | `functional_dashboard_stream_preserves_id_counter_across_events` | event id counter remains monotonic/preserved |
| C-05 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_gateway_ws_upgrade_missing_session_token_returns_unauthorized`
- `cargo test -p tau-gateway functional_gateway_ws_upgrade_includes_session_token_when_present`
- `cargo test -p tau-gateway functional_dashboard_stream_returns_sse_when_requested`
- `cargo test -p tau-gateway functional_dashboard_stream_preserves_id_counter_across_events`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
