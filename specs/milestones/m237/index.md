# M237 - gateway server state modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still contains `GatewayOpenResponsesServerConfig` and `GatewayOpenResponsesServerState` definitions plus core state helper methods. These can be isolated into a dedicated module while preserving root API and runtime behavior.

## Scope
- Move server config/state type definitions + state helper methods into a dedicated module.
- Preserve root-level `GatewayOpenResponsesServerConfig` API path.
- Ratchet root-module size guard.

## Linked Issues
- Epic: #3238
- Story: #3239
- Task: #3240

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_returns_service_snapshot`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_localhost_dev_mode_allows_requests_without_bearer_token`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
