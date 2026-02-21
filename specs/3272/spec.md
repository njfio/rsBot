# Spec: Issue #3272 - move openresponses entry handler to dedicated module

Status: Reviewed

## Problem Statement
`gateway_openresponses.rs` still defines `handle_openresponses`, the endpoint entry handler for `/v1/responses`. This handler can be isolated into a dedicated module without changing behavior.

## Scope
In scope:
- Move `handle_openresponses` into `gateway_openresponses/openresponses_entry_handler.rs`.
- Preserve auth/rate-limit checks, body limit checks, JSON parsing, and stream/non-stream branching.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Endpoint path or payload contract changes.
- Openresponses execution semantics changes.
- Auth/session model changes.

## Acceptance Criteria
### AC-1 openresponses entry behavior remains stable
Given existing openresponses functional/regression tests,
when tests run,
then non-stream and stream=true behavior plus oversized-input rejection remain unchanged.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when root guard runs,
then root line count is under tightened threshold and `handle_openresponses` is no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | non-stream request | `functional_openresponses_endpoint_returns_non_stream_response` | non-stream JSON response contract remains stable |
| C-02 | AC-1 | Functional/Conformance | stream=true request | `functional_openresponses_endpoint_streams_sse_for_stream_true` | stream response emits expected SSE events |
| C-03 | AC-1 | Regression/Conformance | oversized request body | `regression_openresponses_endpoint_rejects_oversized_input` | endpoint returns payload-too-large contract |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_openresponses_endpoint_returns_non_stream_response`
- `cargo test -p tau-gateway functional_openresponses_endpoint_streams_sse_for_stream_true`
- `cargo test -p tau-gateway regression_openresponses_endpoint_rejects_oversized_input`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
