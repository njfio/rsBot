# Spec: Issue #2679 - PRD gateway safety rules and safety test endpoints

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` API contract requires `/gateway/safety/rules` (`GET/PUT`) and `/gateway/safety/test` (`POST`) so operators can inspect/update rule bundles and validate test input against active safety rules. `tau-gateway` currently exposes only `/gateway/safety/policy`, leaving the dashboard safety workflows incomplete.

## Acceptance Criteria

### AC-1 Authenticated operators can read effective safety rules
Given a valid authenticated request,
When `GET /gateway/safety/rules` is called,
Then the gateway returns an effective rules payload with source metadata (`default` or `persisted`) and persistence path.

### AC-2 Authenticated operators can persist safety rules updates
Given a valid authenticated request with a valid rules payload,
When `PUT /gateway/safety/rules` is called,
Then the rules payload is validated, persisted, and returned with `updated=true` metadata.

### AC-3 Invalid safety rules payloads fail closed
Given malformed or invalid rules payloads,
When `PUT /gateway/safety/rules` is called,
Then the gateway returns `400` with deterministic error codes and does not persist rule state.

### AC-4 Authenticated operators can test input against active rules
Given a valid authenticated request,
When `POST /gateway/safety/test` is called with non-empty input,
Then the gateway evaluates prompt-injection and secret-leak rule sets and returns matched rule metadata and reason codes.

### AC-5 Safety test endpoint reflects safety-policy block semantics
Given block mode in persisted safety policy and matching input,
When `POST /gateway/safety/test` is called,
Then the response indicates `blocked=true` according to active safety-policy modes.

### AC-6 Unauthorized or invalid safety test requests are rejected
Given missing/invalid auth or invalid test payload,
When safety rules/test endpoints are called,
Then the gateway returns fail-closed `401`/`400` errors with deterministic codes.

### AC-7 Gateway status discovery advertises safety rules and test endpoints
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `safety_rules_endpoint` and `safety_test_endpoint`.

### AC-8 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add new gateway route templates:
  - `GET/PUT /gateway/safety/rules`
  - `POST /gateway/safety/test`
- Add serialized safety-rule contracts and default rules projection from `tau-safety`.
- Validate safety-rule payloads prior to persistence.
- Persist rules under gateway state (`<state_dir>/openresponses/safety-rules.json`).
- Evaluate test input against active rule bundle and safety policy modes.
- Add status discovery fields for safety rules/test endpoints.
- Add integration/regression tests for auth, validation, persistence, evaluation, and discovery.

### Out of Scope
- Hot-reload of in-flight `Agent` instances from gateway rules updates.
- Full dashboard UI implementation.
- Historical safety-event storage/analytics endpoints.

## Conformance Cases
- C-01 (functional): `GET /gateway/safety/rules` returns effective default rules and metadata.
- C-02 (functional): `PUT /gateway/safety/rules` persists validated rules and subsequent `GET` returns `persisted` source.
- C-03 (regression): invalid `PUT /gateway/safety/rules` payload returns deterministic `400` and does not create persistence file.
- C-04 (functional): `POST /gateway/safety/test` returns matched rules/reason codes for active rule set.
- C-05 (regression): `POST /gateway/safety/test` sets `blocked=true` when matching rules under block policy modes.
- C-06 (regression): unauthorized/invalid safety rules/test requests return fail-closed errors.
- C-07 (regression): `GET /gateway/status` includes `safety_rules_endpoint` and `safety_test_endpoint`.
- C-08 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can manage safety rules and run safety input probes without editing files manually.
- Invalid payloads never mutate persisted safety-rule state.
- Safety endpoint discovery is fully API-driven through `/gateway/status`.
