# Plan: Issue #2708 - Cortex observer coverage for memory-save and worker-progress signals

## Approach
1. Add RED integration/regression tests asserting new event counters in `/cortex/status`.
2. Extend Cortex runtime helper surface with compact recording helpers for new event types.
3. Wire observer event calls into gateway memory-write and external-coding progress/followup handlers.
4. Run scoped verification gates and capture evidence.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/cortex_runtime.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks / Mitigations
- Risk: telemetry I/O failures could affect endpoint reliability.
  - Mitigation: preserve fail-open behavior for observer event recording.
- Risk: status counters drift from malformed artifacts.
  - Mitigation: keep existing malformed-line handling and deterministic fallback diagnostics.

## Interfaces / Contracts
- Existing `GET /cortex/status` response schema remains unchanged; only event counts increase.
- Observer events appended to `openresponses/cortex-observer-events.jsonl` with stable `event_type` values.

## ADR
- Not required. No new dependency or architectural boundary change.
