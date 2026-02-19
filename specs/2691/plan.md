# Plan: Issue #2691 - PRD gateway tools inventory and stats endpoints

## Approach
1. Extend `gateway_openresponses` route constants/router wiring with tools inventory/stats endpoints.
2. Add `tools_runtime` helper module for:
   - Tool inventory collection from configured `GatewayToolRegistrar`.
   - Telemetry JSONL aggregation into per-tool stats with malformed-line accounting.
3. Add handlers with existing auth/error conventions.
4. Extend `/gateway/status` discovery metadata under `gateway.web_ui`.
5. Add tests first (RED), implement handlers (GREEN), then run scoped gates.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `crates/tau-gateway/src/gateway_openresponses/tools_runtime.rs` (new)

## Risks / Mitigations
- Risk: test inventory may be empty under no-op registrar.
  - Mitigation: use fixture tool registrar in tests to assert non-empty deterministic inventory.
- Risk: telemetry payload drift.
  - Mitigation: tolerant parser with defaults + malformed-line counters and diagnostics.
- Risk: accidental behavior drift on existing endpoints.
  - Mitigation: scoped plus full crate test gate before PR.

## Interfaces / Contracts
- `GET /gateway/tools`
  - `200`: `{ schema_version, generated_unix_ms, total_tools, tools: [{name, enabled}] }`
- `GET /gateway/tools/stats`
  - `200`: `{ schema_version, generated_unix_ms, total_tools, total_events, invalid_records, stats: [...], diagnostics: [...] }`

## ADR
- Not required. No dependency/protocol/architecture decision change.
