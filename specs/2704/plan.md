# Plan: Issue #2704 - Cortex observer status endpoint and event tracking

## Approach
1. Add RED tests for authenticated status retrieval, event tracking evidence, unauthorized rejection, missing-artifact fallback, and status discovery metadata.
2. Extend `cortex_runtime` with observer event persistence and status loader helpers.
3. Add `GET /cortex/status` route wiring and status discovery metadata in `gateway_openresponses`.
4. Hook event recording into selected gateway runtime operations (chat/session/external coding agent lifecycle).
5. Run scoped verification gates and capture evidence.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/cortex_runtime.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks / Mitigations
- Risk: observer event logging failures could impact request availability.
  - Mitigation: log persistence is best-effort and fail-open for primary endpoint behavior.
- Risk: status parsing drift from malformed lines.
  - Mitigation: count malformed lines and return diagnostics without hard failure.
- Risk: auth regression.
  - Mitigation: explicit unauthorized regression coverage.

## Interfaces / Contracts
- `GET /cortex/status`
  - `200`: `{ schema_version, generated_unix_ms, state_present, total_events, invalid_events, event_type_counts, recent_events, diagnostics }`
  - `401`: unauthorized.
- Observer events persisted to gateway state dir JSONL with deterministic fields.

## ADR
- Not required. No dependency/protocol/architecture decision change.
