# Plan: Issue #2697 - PRD gateway deploy and stop agent endpoints

## Approach
1. Add RED conformance/regression tests covering deploy success, stop success, unknown-id stop, invalid deploy input, unauthorized access, and status discovery.
2. Add a `deploy_runtime` module in `gateway_openresponses` that persists deterministic deploy agent state under gateway state dir.
3. Wire new routes and status discovery metadata in `gateway_openresponses`.
4. Implement deterministic validation/error mapping and bounded service lifecycle integration.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/deploy_runtime.rs` (new)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks / Mitigations
- Risk: deploying unknown contract shape may drift from UI expectations.
  - Mitigation: explicit response fields and conformance assertions.
- Risk: stop behavior for unknown IDs may become ambiguous.
  - Mitigation: deterministic `agent_not_found` code with regression coverage.
- Risk: auth regressions.
  - Mitigation: explicit unauthorized tests for both endpoints.

## Interfaces / Contracts
- `POST /gateway/deploy`
  - `200`: `{ schema_version, agent_id, status, accepted_unix_ms }`
  - `400`: deterministic validation error for missing/empty `agent_id`.
- `POST /gateway/agents/{agent_id}/stop`
  - `200`: `{ schema_version, agent_id, status, stopped_unix_ms }`
  - `404`: deterministic `agent_not_found`.

## ADR
- Not required. No dependency/protocol/architecture decision change.
