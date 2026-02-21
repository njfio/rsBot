# Plan: Issue #3216 - move /gateway/status handler into status_runtime module

## Approach
1. Tighten `scripts/dev/test-gateway-openresponses-size.sh` threshold and require `status_runtime` wiring; capture RED.
2. Extract `handle_gateway_status` into new `status_runtime.rs` module and wire router imports.
3. Re-run size guard, status integration tests, and quality gates.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/status_runtime.rs` (new)
- `scripts/dev/test-gateway-openresponses-size.sh`
- `specs/milestones/m231/index.md`
- `specs/3216/spec.md`
- `specs/3216/plan.md`
- `specs/3216/tasks.md`

## Risks & Mitigations
- Risk: accidental payload drift in `/gateway/status`.
  - Mitigation: existing integration status tests are mandatory verify gates.
- Risk: threshold too aggressive causing churn.
  - Mitigation: ratchet to value validated by post-extraction line count with minimal buffer.

## Interfaces / Contracts
- `GET /gateway/status` schema and semantics unchanged.
- Existing route constants remain owned by root module.

## ADR
No ADR required (internal module extraction and guard ratchet).
