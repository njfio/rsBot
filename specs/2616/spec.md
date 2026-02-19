# Spec: Issue #2616 - OpenTelemetry export path for production observability

Status: Implemented

## Problem Statement
Tau runtime and gateway components emit JSON diagnostics, but operators currently lack a dedicated OpenTelemetry-compatible export stream for trace/metric ingestion pipelines. We need an opt-in export path that preserves existing telemetry behavior and cost profile by default.

## Acceptance Criteria

### AC-1 CLI/config exposes an opt-in OpenTelemetry export path
Given local runtime or gateway contract runner execution,
When operators provide an OpenTelemetry export log path,
Then startup wiring propagates that path into runtime/gateway observability components without changing defaults when unset.

### AC-2 Prompt runtime telemetry emits OpenTelemetry-compatible trace and metric records
Given prompt telemetry logging with OpenTelemetry export enabled,
When an agent prompt run completes,
Then Tau appends OpenTelemetry-compatible trace and metric records for that run while preserving existing `prompt_telemetry_v1` output.

### AC-3 Gateway runtime emits OpenTelemetry-compatible cycle trace and metric records
Given gateway contract runner execution with OpenTelemetry export enabled,
When a gateway cycle report is emitted,
Then Tau appends OpenTelemetry-compatible trace and metric records for cycle health/counters in addition to existing gateway runtime event logs.

### AC-4 Scoped verification is green
Given the observability export changes,
When scoped formatting, linting, and targeted tests run,
Then all checks pass.

## Scope

### In Scope
- Add optional CLI/env flag for OpenTelemetry export log path.
- Propagate OpenTelemetry export path into local runtime prompt telemetry logger and gateway contract runner config.
- Emit OpenTelemetry-compatible trace + metric JSON records for prompt telemetry and gateway runtime cycles.
- Add/extend tests for runtime and gateway OpenTelemetry export behavior.

### Out of Scope
- Direct OTLP network exporter/transporter implementation.
- New OpenTelemetry crate dependencies.
- Replacing or removing existing telemetry/event log formats.

## Conformance Cases
- C-01 (unit): CLI parsing and transport config propagation include OpenTelemetry export path.
- C-02 (functional): prompt telemetry logger writes OpenTelemetry trace + metric records when export path is configured.
- C-03 (regression): prompt telemetry logger keeps legacy telemetry output unchanged and does not emit OTel records when export path is unset.
- C-04 (integration): gateway runtime appends OpenTelemetry cycle trace + metric records alongside runtime events when configured.
- C-05 (verify): `cargo fmt --check`, `cargo clippy -p tau-runtime -p tau-gateway -p tau-onboarding -p tau-cli -- -D warnings`, `cargo test -p tau-runtime observability_loggers_runtime`, `cargo test -p tau-gateway gateway_runtime`, and `cargo test -p tau-onboarding gateway_contract_runner` pass.

## Success Metrics / Observable Signals
- Operators can set one OpenTelemetry export path and ingest runtime/gateway records with stable schema fields.
- Existing telemetry and gateway runtime logs remain backward compatible.
- No default-path behavior changes when OpenTelemetry export is not configured.
