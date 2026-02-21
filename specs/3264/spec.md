# Spec: Issue #3264 - move stream_openresponses handler to dedicated module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still includes `stream_openresponses`, which can be isolated into a dedicated module without changing endpoint behavior. Keeping stream orchestration in root increases module density and weakens ownership boundaries.

## Scope
In scope:
- Move `stream_openresponses` from root into `gateway_openresponses/stream_response_handler.rs`.
- Preserve openresponses streaming and non-streaming response contracts.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Endpoint path/payload changes.
- Execution semantics changes in `execute_openresponses_request`.
- Provider/auth model changes.

## Acceptance Criteria
### AC-1 openresponses stream/non-stream contracts remain stable
Given existing openresponses functional/regression tests,
when tests run,
then non-stream responses remain stable and stream=true requests continue returning SSE frames with expected events.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when root guard runs,
then root line count is under tightened threshold and `stream_openresponses` is no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | non-stream openresponses request | `functional_openresponses_endpoint_returns_non_stream_response` | non-stream JSON response contract remains stable |
| C-02 | AC-1 | Functional/Conformance | stream=true openresponses request | `functional_openresponses_endpoint_streams_sse_for_stream_true` | stream endpoint returns SSE content with expected response events |
| C-03 | AC-1 | Regression/Conformance | malformed openresponses JSON request | `regression_openresponses_endpoint_rejects_malformed_json_body` | malformed-json failure contract remains stable |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_openresponses_endpoint_returns_non_stream_response`
- `cargo test -p tau-gateway functional_openresponses_endpoint_streams_sse_for_stream_true`
- `cargo test -p tau-gateway regression_openresponses_endpoint_rejects_malformed_json_body`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
