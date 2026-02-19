# Spec: Issue #2688 - PRD gateway training rollouts and config endpoints

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` API contract requires training control/history endpoints beyond status:
- `GET /gateway/training/rollouts`
- `PATCH /gateway/training/config`

`tau-gateway` currently exposes only `GET /gateway/training/status`, which blocks Training & RL dashboard server-function parity.

## Acceptance Criteria

### AC-1 Authenticated operators can fetch paginated training rollouts
Given a valid authenticated request,
When `GET /gateway/training/rollouts` is called,
Then the gateway returns deterministic rollout history payload with pagination metadata.

### AC-2 Rollouts endpoint fails open with deterministic fallback when artifacts are missing/malformed
Given missing or partially malformed rollout artifacts,
When `GET /gateway/training/rollouts` is called,
Then the endpoint returns `200` with deterministic diagnostics and valid pagination payload.

### AC-3 Rollouts endpoint validates pagination query input
Given invalid query parameters (`page` / `per_page` bounds),
When `GET /gateway/training/rollouts` is called,
Then the gateway returns deterministic fail-closed `400` with actionable error code.

### AC-4 Authenticated operators can patch training config overrides
Given a valid authenticated patch payload,
When `PATCH /gateway/training/config` is called,
Then the gateway persists deterministic training config overrides and returns accepted/applied metadata.

### AC-5 Training config endpoint validates payload and rejects unsupported/invalid updates
Given missing supported fields or invalid values,
When `PATCH /gateway/training/config` is called,
Then the gateway returns deterministic `400` response without mutating persisted overrides.

### AC-6 Unauthorized training endpoint requests are rejected
Given missing or invalid auth,
When `GET /gateway/training/rollouts` or `PATCH /gateway/training/config` are called,
Then the gateway returns fail-closed `401`.

### AC-7 Gateway status discovery advertises training rollouts/config endpoints
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `training_rollouts_endpoint` and `training_config_endpoint`.

### AC-8 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add gateway routes:
  - `GET /gateway/training/rollouts`
  - `PATCH /gateway/training/config`
- Add deterministic training rollouts artifact parsing with bounded pagination.
- Add deterministic training config override persistence contract.
- Add status discovery fields under `gateway.web_ui`.
- Add conformance/regression tests for success, fallback, validation, and auth.

### Out of Scope
- Dashboard UI implementation.
- Runtime hot-application of patched training config values.
- RL algorithm/store schema redesign.

## Conformance Cases
- C-01 (integration): `GET /gateway/training/rollouts` returns paginated rollout records and status discovery fields.
- C-02 (regression): missing rollout artifact returns deterministic empty payload with diagnostics.
- C-03 (regression): malformed rollout artifact lines are tolerated and counted in diagnostics.
- C-04 (regression): invalid pagination query returns deterministic `400`.
- C-05 (integration): `PATCH /gateway/training/config` persists supported overrides and returns accepted/applied contract.
- C-06 (regression): invalid or unsupported training config patch payload returns deterministic `400` without mutation.
- C-07 (regression): unauthorized training rollout/config requests return `401`.
- C-08 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Training dashboard integrations can fetch rollout history and submit training config updates via dedicated endpoints.
- Missing/malformed training artifacts no longer crash or break endpoint contracts.
- Endpoint discovery remains API-driven via `/gateway/status`.
