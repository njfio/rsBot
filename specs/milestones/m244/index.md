# M244 - gateway auth-session handler modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still contains the auth-session request handler used for password-session token issuance. Extracting this handler keeps root focused on orchestration and further reduces module density.

## Scope
- Move `handle_gateway_auth_session` into a dedicated module.
- Preserve password-session auth contract and rate-limit behavior.
- Ratchet root-module size/ownership guard.

## Linked Issues
- Epic: #3266
- Story: #3267
- Task: #3268

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway functional_gateway_auth_session_endpoint_issues_bearer_for_password_mode`
- `cargo test -p tau-gateway regression_gateway_auth_session_rejects_invalid_password`
- `cargo test -p tau-gateway regression_gateway_password_session_token_expires_and_fails_closed`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
