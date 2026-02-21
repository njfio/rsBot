# M233 - gateway multi-channel status type modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still contains multi-channel status/report structs and runtime-state DTOs that are only used by `multi_channel_status.rs`. Keeping these types in root inflates module size and couples status projection internals to the root router module.

## Scope
- Move multi-channel status/report structs + defaults into `gateway_openresponses/multi_channel_status.rs`.
- Move multi-channel runtime-state DTOs used for state/event parsing into `multi_channel_status.rs`.
- Preserve `/gateway/status` multi-channel payload semantics and existing tests.
- Ratchet root module size guard.

## Linked Issues
- Epic: #3222
- Story: #3223
- Task: #3224

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway unit_collect_gateway_multi_channel_status_report_composes_runtime_and_connector_fields`
- `cargo test -p tau-gateway regression_collect_gateway_multi_channel_status_report_defaults_when_state_is_missing`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_expanded_multi_channel_health_payload`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
