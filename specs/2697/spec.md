# Spec: Issue #2697 - PRD gateway deploy and stop agent endpoints

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` defines deploy/stop operator actions but `tau-gateway` currently has no dedicated API endpoints for:
- `POST /gateway/deploy`
- `POST /gateway/agents/{agent_id}/stop`

## Acceptance Criteria

### AC-1 Authenticated operators can request deploy
Given a valid authenticated request with a valid `agent_id`,
When `POST /gateway/deploy` is called,
Then the gateway records deterministic deploy state and returns deploy-accepted payload.

### AC-2 Authenticated operators can stop a known deployed agent
Given a valid authenticated request and a known `agent_id`,
When `POST /gateway/agents/{agent_id}/stop` is called,
Then the gateway records deterministic stopped state and returns stop payload.

### AC-3 Unknown agent stop requests return deterministic not-found error
Given a valid authenticated request with an unknown `agent_id`,
When `POST /gateway/agents/{agent_id}/stop` is called,
Then the gateway returns deterministic `404` with stable `agent_not_found` error code.

### AC-4 Unauthorized deploy/stop requests are rejected
Given missing or invalid auth,
When deploy/stop endpoints are called,
Then the gateway fails closed with `401`.

### AC-5 Gateway status discovery advertises deploy/stop endpoints
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `deploy_endpoint` and `agent_stop_endpoint_template`.

### AC-6 Invalid deploy requests are rejected deterministically
Given an authenticated request without a valid non-empty `agent_id`,
When `POST /gateway/deploy` is called,
Then the gateway returns deterministic `400` validation error.

### AC-7 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add gateway routes:
  - `POST /gateway/deploy`
  - `POST /gateway/agents/{agent_id}/stop`
- Persist deterministic deploy/stop runtime state in gateway state dir.
- Add status discovery metadata for deploy/stop endpoints.
- Add conformance/regression tests for success, unknown-id, validation, and auth behavior.

### Out of Scope
- Full multi-agent process orchestration.
- Runtime supervisor refactor for independent process pools.
- Dashboard UI implementation.

## Conformance Cases
- C-01 (integration): deploy endpoint accepts valid authenticated request and returns deterministic payload.
- C-02 (integration): stop endpoint stops known deployed agent and returns deterministic payload.
- C-03 (regression): unknown stop id returns deterministic `404` `agent_not_found`.
- C-04 (regression): unauthorized deploy/stop requests return `401`.
- C-05 (regression): status discovery includes deploy/stop endpoint metadata.
- C-06 (regression): deploy endpoint rejects invalid payload with deterministic `400`.
- C-07 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can trigger deploy/stop actions through dedicated gateway API surfaces.
- Dashboard endpoint discovery for deploy/stop is API-driven through `/gateway/status`.
- Deploy/stop contracts are deterministic and test-backed.
