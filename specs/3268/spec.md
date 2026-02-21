# Spec: Issue #3268 - move auth-session handler to dedicated module

Status: Implemented

## Problem Statement
`gateway_openresponses.rs` still defines `handle_gateway_auth_session`. This endpoint-specific handler can be isolated into a dedicated module without changing the auth/session endpoint contract.

## Scope
In scope:
- Move `handle_gateway_auth_session` into `gateway_openresponses/auth_session_handler.rs`.
- Preserve password-session mode behavior, rate-limiting, and malformed-json handling.
- Ratchet and enforce root-module size/ownership guard.

Out of scope:
- Auth/session token issuance semantics.
- Endpoint path/payload changes.
- Auth mode model changes.

## Acceptance Criteria
### AC-1 auth-session endpoint behavior remains stable
Given existing auth-session functional/regression tests,
when tests run,
then password-session issuance and error contracts remain unchanged.

### AC-2 root module ownership boundaries improve
Given refactored module layout,
when root guard runs,
then root line count is under tightened threshold and `handle_gateway_auth_session` is no longer declared in root.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional/Conformance | password-session mode with valid password | `functional_gateway_auth_session_endpoint_issues_bearer_for_password_mode` | auth session endpoint issues bearer token and authorizes follow-up call |
| C-02 | AC-1 | Regression/Conformance | password-session request with invalid password | `regression_gateway_auth_session_rejects_invalid_password` | endpoint fails closed with invalid-credentials contract |
| C-03 | AC-1 | Regression/Conformance | password-session issued token after ttl expiry | `regression_gateway_password_session_token_expires_and_fails_closed` | expired session token is rejected by authorized endpoint |
| C-04 | AC-2 | Functional/Regression | repo checkout | `scripts/dev/test-gateway-openresponses-size.sh` | tightened threshold + ownership checks pass |

## Success Metrics / Observable Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_gateway_auth_session_endpoint_issues_bearer_for_password_mode`
- `cargo test -p tau-gateway regression_gateway_auth_session_rejects_invalid_password`
- `cargo test -p tau-gateway regression_gateway_password_session_token_expires_and_fails_closed`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
