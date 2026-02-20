# Plan: Issue #2701 - Cortex admin chat SSE endpoint foundation

## Approach
1. Add RED conformance/regression tests for authenticated SSE flow, validation/auth errors, and status discovery metadata.
2. Implement a dedicated `cortex_runtime` handler module with deterministic SSE event contract and request validation.
3. Wire `POST /cortex/chat` route and status discovery metadata in `gateway_openresponses`.
4. Run scoped verification gates and capture evidence.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/cortex_runtime.rs` (new)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks / Mitigations
- Risk: SSE contract drift in future Cortex iterations.
  - Mitigation: explicit event names/payload assertions in conformance tests.
- Risk: auth bypass regressions.
  - Mitigation: dedicated unauthorized regression test.
- Risk: malformed payload acceptance.
  - Mitigation: explicit validation for non-empty input and deterministic error code.

## Interfaces / Contracts
- `POST /cortex/chat`
  - `200` SSE with events:
    - `cortex.response.created`
    - `cortex.response.output_text.delta`
    - `cortex.response.output_text.done`
    - `done`
  - `400` invalid payload (`invalid_cortex_input`)
  - `401` unauthorized

## ADR
- Not required. No dependency/protocol/architecture decision change.
