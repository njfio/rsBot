# Plan: Issue #2679 - PRD gateway safety rules and safety test endpoints

## Approach
1. Extend `tau-safety` with serializable safety-rule contracts, default rule-set projection, validation, and runtime scan helpers for rule-set evaluation.
2. Re-export new safety-rule contracts/functions from `tau-agent-core` so gateway can consume without new direct dependencies.
3. Add gateway constants/routes for `/gateway/safety/rules` and `/gateway/safety/test`.
4. Implement handlers:
   - `GET /gateway/safety/rules`: auth + effective rules read (`persisted` fallback to default).
   - `PUT /gateway/safety/rules`: auth + payload validation + atomic persistence.
   - `POST /gateway/safety/test`: auth + payload validation + active policy/rules evaluation + blocked semantic.
5. Update `/gateway/status` web UI discovery payload with new safety endpoint fields.
6. Add RED-first integration/regression tests for C-01..C-07.

## Affected Modules
- `crates/tau-safety/src/lib.rs`
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/types.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/milestones/m110/index.md`

## Risks / Mitigations
- Risk: regex rule payloads may include invalid patterns.
  - Mitigation: explicit validation with deterministic `invalid_safety_rules` response.
- Risk: rule evaluation diverges from defaults.
  - Mitigation: default rules derived from `tau-safety` canonical constants.
- Risk: endpoint sprawl breaks status contract.
  - Mitigation: additive status fields only with regression assertions.

## Interfaces / Contracts
- `GET /gateway/safety/rules` -> `{ rules, source, path }`
- `PUT /gateway/safety/rules` -> `{ updated, rules, source, path, updated_unix_ms }`
- `POST /gateway/safety/test` -> `{ blocked, reason_codes, matches, source, policy_source }`
- Persistence file:
  - `<gateway_state_dir>/openresponses/safety-rules.json`

## ADR
- Not required (bounded additive API slice, no dependency/protocol introduction).
- Human review requested in PR because this is a P1 multi-module change.
