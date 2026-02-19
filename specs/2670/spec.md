# Spec: Issue #2670 - PRD channel lifecycle action gateway endpoint

Status: Implemented

## Problem Statement
`specs/tau-ops-dashboard-prd.md` requires `POST /gateway/channels/{channel}/lifecycle` so operators can trigger lifecycle actions for Telegram/Discord/WhatsApp from the dashboard. `tau-gateway` currently exposes multi-channel status but does not provide a lifecycle action endpoint.

## Acceptance Criteria

### AC-1 Authenticated operators can trigger supported lifecycle actions
Given a valid authenticated operator request,
When `POST /gateway/channels/{channel}/lifecycle` is called with a supported channel/action,
Then the gateway executes the mapped `tau-multi-channel` lifecycle action and returns a structured lifecycle report payload.

### AC-2 Invalid channel/action inputs fail closed with deterministic errors
Given malformed or unsupported channel/action inputs,
When the lifecycle endpoint is called,
Then the gateway returns `400` with explicit error codes and does not execute lifecycle state mutations.

### AC-3 Unauthorized requests are rejected
Given missing or invalid auth credentials,
When lifecycle actions are requested,
Then the gateway returns `401` and performs no lifecycle action.

### AC-4 Gateway status discovery advertises the lifecycle endpoint
Given an authenticated status inspection request,
When `GET /gateway/status` is called,
Then the payload includes the new channel lifecycle endpoint template for dashboard discovery.

### AC-5 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- New gateway route template: `POST /gateway/channels/{channel}/lifecycle`.
- Request parsing/validation for `channel` and lifecycle `action` (`status`, `login`, `logout`, `probe`).
- Lifecycle action execution through `tau-multi-channel` runtime command APIs.
- Status payload update so dashboard can discover lifecycle endpoint.
- Integration/regression tests for auth, validation, and endpoint contract behavior.

### Out of Scope
- Full channel credential management UI flows.
- Additional PRD endpoint families (`/gateway/config`, safety, audit, training, jobs, deploy, stop-agent).
- Leptos dashboard crate creation/rendering.

## Conformance Cases
- C-01 (functional): `POST /gateway/channels/telegram/lifecycle` with `{"action":"logout"}` returns `200` lifecycle report and persists lifecycle state under multi-channel state root.
- C-02 (functional): `POST /gateway/channels/discord/lifecycle` with `{"action":"status"}` returns `200` report payload with action/channel identity.
- C-03 (regression): unsupported channel path segment returns `400` with `invalid_channel` error code.
- C-04 (regression): unsupported action value returns `400` with `invalid_lifecycle_action` error code.
- C-05 (regression): unauthorized lifecycle requests return `401` and do not execute.
- C-06 (regression): `GET /gateway/status` includes `channel_lifecycle_endpoint` while preserving existing gateway web UI endpoint fields.
- C-07 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can execute lifecycle actions without leaving gateway/dashboard surfaces.
- Validation failures are explicit and deterministic for bad channel/action inputs.
- Existing gateway status/web UI contract remains compatible while exposing the new endpoint.
