# M232 - gateway compat/telemetry state modularization

Status: In Progress

## Context
`gateway_openresponses.rs` still contains OpenAI compatibility runtime counters, UI telemetry runtime counters, and related state mutation/reporting methods. This logic can be isolated while preserving status endpoint output.

## Scope
- Extract compat/telemetry runtime state types and helper methods into dedicated module.
- Preserve `/gateway/status` compat and telemetry counter payload behavior.
- Ratchet root-module size guard threshold and keep it passing.

## Linked Issues
- Epic: #3218
- Story: #3219
- Task: #3220

## Success Signals
- `scripts/dev/test-gateway-openresponses-size.sh`
- `cargo test -p tau-gateway integration_gateway_status_endpoint_reports_openai_compat_runtime_counters`
- `cargo test -p tau-gateway integration_gateway_ui_telemetry_endpoint_persists_events_and_status_counters`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
