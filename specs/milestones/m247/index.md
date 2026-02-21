# M247 - gateway root helper utilities modularization

Status: In Progress

## Context
After prior extractions, `gateway_openresponses.rs` retains only utility helpers. Moving these into a dedicated module keeps the root as pure module wiring and route assembly.

## Scope
- Move `derive_gateway_preflight_token_limit` and `validate_gateway_openresponses_bind` into a utility module.
- Preserve helper behavior and test contracts.
- Ratchet root-module size/ownership guard.

## Linked Issues
- Epic: #3278
- Story: #3279
- Task: #3280

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway regression_validate_gateway_openresponses_bind_rejects_invalid_socket_address`
- `cargo test -p tau-gateway integration_spec_c01_openresponses_request_persists_session_usage_summary`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
