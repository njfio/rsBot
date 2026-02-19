# Spec: Issue #2676 - PRD gateway safety policy GET/PUT endpoint

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` requires `/gateway/safety/policy` (`GET/PUT`) so operators can inspect and update safety-policy settings from dashboard workflows. `tau-gateway` currently does not expose this endpoint, preventing API-driven safety policy management.

## Acceptance Criteria

### AC-1 Authenticated operators can read effective safety policy
Given a valid authenticated request,
When `GET /gateway/safety/policy` is called,
Then the gateway returns effective policy payload, persistence path, and source metadata.

### AC-2 Authenticated operators can persist policy updates
Given a valid authenticated request with a valid `SafetyPolicy` payload,
When `PUT /gateway/safety/policy` is called,
Then the policy is persisted to gateway state and returned in the response.

### AC-3 Invalid policy payloads fail closed
Given malformed or invalid policy payloads,
When `PUT /gateway/safety/policy` is called,
Then the gateway returns `400` with deterministic error codes and does not persist state.

### AC-4 Unauthorized safety policy requests are rejected
Given missing or invalid credentials,
When `GET` or `PUT /gateway/safety/policy` is called,
Then the gateway returns `401` and does not expose/mutate policy state.

### AC-5 Gateway status discovery includes safety policy endpoint
Given an authenticated status request,
When `GET /gateway/status` is called,
Then the payload advertises `/gateway/safety/policy` for dashboard discovery.

### AC-6 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- New gateway route template: `/gateway/safety/policy` (`GET`, `PUT`).
- Effective policy read behavior (`persisted` if present, otherwise default).
- Policy write behavior with bounded validation and atomic persistence.
- Status payload discovery update for safety policy endpoint.
- Integration/regression tests for auth, validation, persistence, and discovery.

### Out of Scope
- Safety rules CRUD (`/gateway/safety/rules`) and safety test endpoint (`/gateway/safety/test`).
- Runtime in-memory live policy hot-reload for active agent sessions.
- Leptos UI implementation.

## Conformance Cases
- C-01 (functional): `GET /gateway/safety/policy` returns effective policy and source metadata.
- C-02 (functional): `PUT /gateway/safety/policy` with valid `SafetyPolicy` persists and returns updated policy.
- C-03 (regression): `PUT` rejects invalid policy values with `400` and deterministic error code.
- C-04 (regression): unauthorized `GET`/`PUT` returns `401`.
- C-05 (regression): `GET /gateway/status` includes `safety_policy_endpoint` while preserving existing endpoint fields.
- C-06 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can inspect and update safety policy through gateway API without manual file edits.
- Invalid policy payloads are rejected deterministically without partial writes.
- Gateway status discovery includes safety policy endpoint for dashboard wiring.
