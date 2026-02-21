# Spec: Issue #3276 - move openresponses execution handler to dedicated module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still defines `execute_openresponses_request`, the largest execution-path helper in root. It can be extracted into a dedicated module without behavior changes.

## Scope
In scope:
- Move `execute_openresponses_request` into `gateway_openresponses/openresponses_execution_handler.rs`.
- Preserve response generation, stream delta emission, and usage persistence semantics.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Request/response schema changes.
- Model/provider semantics changes.
- Session storage design changes.

## Acceptance Criteria
### AC-1 openresponses execution behavior remains stable
Given existing openresponses functional/integration tests,
when tests run,
then non-stream and stream responses plus usage persistence contracts remain unchanged.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when root guard runs,
then root line count is under tightened threshold and `execute_openresponses_request` is no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | non-stream openresponses request | `functional_openresponses_endpoint_returns_non_stream_response` | non-stream response contract remains stable |
| C-02 | AC-1 | Functional/Conformance | stream=true openresponses request | `functional_openresponses_endpoint_streams_sse_for_stream_true` | stream response events remain stable |
| C-03 | AC-1 | Integration/Conformance | usage summary persistence scenario | `integration_spec_c01_openresponses_request_persists_session_usage_summary` | usage summary persistence contract remains stable |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_openresponses_endpoint_returns_non_stream_response`
- `cargo test -p tau-gateway functional_openresponses_endpoint_streams_sse_for_stream_true`
- `cargo test -p tau-gateway integration_spec_c01_openresponses_request_persists_session_usage_summary`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
