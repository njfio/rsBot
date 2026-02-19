# Plan: Issue #2616 - OpenTelemetry export path for production observability

## Approach
1. Add RED tests for CLI/config propagation and runtime/gateway OpenTelemetry export output expectations.
2. Add optional CLI flag/env for OpenTelemetry export log path.
3. Extend prompt telemetry logger with optional OpenTelemetry export output (trace + metrics) while preserving current telemetry record format.
4. Extend gateway runtime config and cycle-report emission with optional OpenTelemetry export output (trace + metrics).
5. Update onboarding transport/runtime wiring to pass the OpenTelemetry export path.
6. Run scoped verify commands and map results to AC/C cases.

## Affected Modules
- `crates/tau-cli/src/cli_args/gateway_daemon_flags.rs`
- `crates/tau-onboarding/src/startup_local_runtime.rs`
- `crates/tau-onboarding/src/startup_transport_modes.rs`
- `crates/tau-onboarding/src/startup_transport_modes/tests.rs`
- `crates/tau-runtime/src/observability_loggers_runtime.rs`
- `crates/tau-gateway/src/gateway_runtime.rs`
- `specs/2616/spec.md`
- `specs/2616/plan.md`
- `specs/2616/tasks.md`

## Risks / Mitigations
- Risk: format drift or unexpected schema changes for existing telemetry consumers.
  - Mitigation: keep existing telemetry/event logs untouched and add OTel export as additive path only.
- Risk: runtime overhead from additional writes.
  - Mitigation: export remains fully opt-in; no write path added when unset.
- Risk: partial config propagation between CLI and runtime/gateway wiring.
  - Mitigation: add targeted propagation tests in onboarding.

## Interfaces / Contracts
- New CLI flag/env:
  - `--otel-export-log`
  - `TAU_OTEL_EXPORT_LOG`
- Prompt telemetry logger:
  - new optional OTel export path parameter; emits additive OTel-compatible records.
- Gateway runtime config:
  - new optional `otel_export_log` path consumed by cycle report writer.

## ADR
- Not required: no new dependencies, wire/protocol version changes, or architecture boundary changes.
