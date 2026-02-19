# Spec: Issue #2685 - PRD gateway training status endpoint

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` API contract includes `GET /gateway/training/status` so operators can query live training state through an authenticated dashboard API. `tau-gateway` currently exposes training status inside broader dashboard/status payloads but lacks a dedicated training status endpoint for dashboard server-function wiring.

## Acceptance Criteria

### AC-1 Authenticated operators can fetch training status through a dedicated endpoint
Given a valid authenticated request,
When `GET /gateway/training/status` is called,
Then the gateway returns a deterministic training status payload derived from persisted training runtime artifacts.

### AC-2 Training status endpoint fails open with deterministic unavailable payload
Given missing or unreadable training runtime status artifacts,
When `GET /gateway/training/status` is called,
Then the endpoint returns `200` with `status_present=false` and deterministic diagnostic metadata.

### AC-3 Unauthorized training status requests are rejected
Given missing or invalid auth,
When `GET /gateway/training/status` is called,
Then the gateway returns fail-closed `401`.

### AC-4 Gateway status discovery advertises training status endpoint
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `training_status_endpoint`.

### AC-5 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add new gateway route template:
  - `GET /gateway/training/status`
- Reuse persisted training report loading behavior from existing dashboard status runtime artifacts.
- Add status discovery field for training status endpoint.
- Add integration/regression tests for success, unavailable fallback, auth, and discovery.

### Out of Scope
- `GET /gateway/training/rollouts`
- `PATCH /gateway/training/config`
- Training runtime state persistence format changes.

## Conformance Cases
- C-01 (functional): `GET /gateway/training/status` returns parsed training status when artifact exists.
- C-02 (regression): `GET /gateway/training/status` returns deterministic unavailable payload when artifact is missing.
- C-03 (regression): unauthorized training status requests return `401`.
- C-04 (regression): `GET /gateway/status` includes `training_status_endpoint`.
- C-05 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Dashboard operators can query training status through a dedicated authenticated endpoint.
- Missing training artifacts no longer require callers to infer status from unrelated payloads.
- Endpoint discovery remains API-driven via `/gateway/status`.
