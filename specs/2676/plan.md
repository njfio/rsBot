# Plan: Issue #2676 - PRD gateway safety policy GET/PUT endpoint

## Approach
1. Add route constant + router wiring for `GET/PUT /gateway/safety/policy`.
2. Add a typed request payload in `types.rs` (`SafetyPolicy` wrapper body).
3. Implement `GET` handler:
   - authorize request,
   - read persisted policy file if present,
   - fallback to `SafetyPolicy::default()` when absent,
   - return source metadata (`persisted` vs `default`).
4. Implement `PUT` handler:
   - authorize request,
   - parse/validate payload,
   - persist policy atomically to gateway state file,
   - return persisted policy with metadata.
5. Update `/gateway/status` web UI discovery payload with safety policy endpoint.
6. Add RED-first integration/regression tests for C-01..C-05.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m110/index.md`

## Risks / Mitigations
- Risk: invalid payload persistence can corrupt policy state.
  - Mitigation: validate required tokens/fields before writing; reject with `400`.
- Risk: read fallback ambiguity.
  - Mitigation: response includes explicit `source` and `path` metadata.
- Risk: breaking status discovery schema.
  - Mitigation: additive field only + regression assertion on existing endpoint map.

## Interfaces / Contracts
- Endpoint: `GET /gateway/safety/policy`
- Endpoint: `PUT /gateway/safety/policy`
- Request payload:
  - `policy: SafetyPolicy`
- Persistence file:
  - `<gateway_state_dir>/openresponses/safety-policy.json`

## ADR
- Not required (bounded endpoint extension without dependency/protocol changes).
