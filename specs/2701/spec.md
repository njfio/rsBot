# Spec: Issue #2701 - Cortex admin chat SSE endpoint foundation

Status: Implemented

## Problem Statement
`tasks/spacebot-comparison.md` gap G3 calls for a Cortex observer surface including an admin chat endpoint. `tau-gateway` currently has no Cortex API surface.

## Acceptance Criteria

### AC-1 Authenticated operators can call Cortex admin chat
Given a valid authenticated request with non-empty input,
When `POST /cortex/chat` is called,
Then the gateway returns an SSE response with deterministic Cortex event schema.

### AC-2 Cortex admin chat returns deterministic completion signaling
Given an authenticated valid request,
When the Cortex stream is consumed,
Then it emits deterministic creation/content/done events and terminates with standard `done` frame.

### AC-3 Unauthorized Cortex admin chat requests are rejected
Given missing or invalid auth,
When `POST /cortex/chat` is called,
Then the gateway fails closed with `401`.

### AC-4 Invalid Cortex admin chat payloads are rejected
Given an authenticated request with empty input,
When `POST /cortex/chat` is called,
Then the gateway returns deterministic `400` with stable validation error code.

### AC-5 Gateway status discovery advertises Cortex admin chat endpoint
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `cortex_chat_endpoint`.

### AC-6 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add endpoint:
  - `POST /cortex/chat` (SSE)
- Deterministic streaming event contract for Cortex admin chat foundation.
- Fail-closed auth and deterministic request validation.
- Status discovery metadata update.
- Conformance/regression tests for success and failure behavior.

### Out of Scope
- Full Cortex cross-session reasoning runtime.
- Bulletin generation and global prompt injection.
- UI implementation for Cortex chat.

## Conformance Cases
- C-01 (integration): authenticated `POST /cortex/chat` returns SSE with deterministic Cortex event frames.
- C-02 (integration): Cortex stream includes deterministic terminal `done` frame.
- C-03 (regression): unauthorized request returns `401`.
- C-04 (regression): empty input request returns deterministic `400` validation error.
- C-05 (regression): `/gateway/status` includes `cortex_chat_endpoint` discovery field.
- C-06 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can invoke Cortex admin chat through a dedicated authenticated SSE endpoint.
- Endpoint discovery is exposed via `/gateway/status`.
- Contract is deterministic and conformance-covered.
