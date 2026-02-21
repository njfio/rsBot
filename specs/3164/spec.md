# Spec: Issue #3164 - tau-training-proxy malformed-header and attribution-log resilience conformance

Status: Accepted

## Problem Statement
`tau-training-proxy` has baseline request/attribution tests, but explicit conformance for malformed attribution headers and log append recovery is still thin. This leaves quality evidence gaps versus the repository quality tracker expectations.

## Scope
In scope:
- Add malformed attribution conformance tests for empty required headers and invalid UTF-8 header input.
- Add a recovery conformance test that proves attribution logging recreates missing parent directories.
- Add a persistence conformance test that proves existing attribution log entries are preserved and new entries append.
- Apply the minimal runtime implementation change required for recovery behavior.

Out of scope:
- API contract changes for training-proxy endpoints.
- New dependencies or protocol/schema updates.
- Non-training-proxy module changes.

## Acceptance Criteria
### AC-1 Malformed attribution headers fail with deterministic parse errors
Given request headers for `parse_training_proxy_attribution`,
when required attribution headers are empty or optional headers are invalid UTF-8,
then parsing fails with deterministic header-specific errors.

### AC-2 Attribution logging recovers after missing training directory
Given a valid proxy request and upstream success response,
when the training attribution directory is deleted after startup,
then the proxy recreates the directory and appends an attribution record.

### AC-3 Attribution log append preserves existing entries
Given an existing attribution log file with prior entries,
when a new proxied request completes,
then prior entries remain and exactly one new record is appended.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Unit/Conformance | required attribution headers include whitespace-only rollout id | parse attribution | parse fails with `header 'x-rollout-id' cannot be empty` |
| C-02 | AC-1 | Unit/Conformance | optional trace header contains invalid UTF-8 bytes | parse attribution | parse fails with `header 'x-trace-id' must be valid utf-8` |
| C-03 | AC-2 | Integration/Conformance | startup state exists, then training directory removed | proxy handles chat completion | response succeeds and attribution log is recreated with new entry |
| C-04 | AC-3 | Integration/Conformance | attribution log already contains prior line | proxy handles chat completion | log retains prior line and appends one new JSON line |

## Success Metrics / Observable Signals
- `cargo test -p tau-training-proxy spec_3164 -- --test-threads=1`
- `cargo test -p tau-training-proxy`
- `cargo fmt --check`
- `cargo clippy -p tau-training-proxy -- -D warnings`
