# Spec: Issue #2704 - Cortex observer status endpoint and event tracking

Status: Implemented

## Problem Statement
Cortex foundation now exposes `POST /cortex/chat`, but there is no observer runtime status surface or event tracking report to support cross-session operational visibility from `tasks/spacebot-comparison.md` G3.

## Acceptance Criteria

### AC-1 Authenticated operators can inspect Cortex observer status
Given a valid authenticated request,
When `GET /cortex/status` is called,
Then the gateway returns deterministic Cortex observer status payload.

### AC-2 Cortex observer tracks key runtime events
Given gateway runtime actions occur,
When Cortex status is inspected,
Then event counters and recent event entries reflect tracked operations.

### AC-3 Unauthorized Cortex status requests are rejected
Given missing or invalid auth,
When `GET /cortex/status` is called,
Then the gateway fails closed with `401`.

### AC-4 Missing or empty observer artifacts return deterministic fallback
Given no observer events artifact exists yet,
When `GET /cortex/status` is called with valid auth,
Then the gateway returns `200` with deterministic empty-state diagnostics.

### AC-5 Gateway status discovery advertises Cortex status endpoint
Given an authenticated status request,
When `GET /gateway/status` is called,
Then `gateway.web_ui` includes `cortex_status_endpoint`.

### AC-6 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Scope

### In Scope
- Add endpoint:
  - `GET /cortex/status`
- Add deterministic observer event tracking persistence for selected gateway operations:
  - `cortex.chat.request`
  - `session.append`
  - `session.reset`
  - `external_coding_agent.session_opened`
  - `external_coding_agent.session_closed`
- Add gateway status discovery metadata for Cortex status endpoint.
- Add conformance/regression tests for auth, fallback, and tracking behavior.

### Out of Scope
- Full Cortex bulletin generation and global prompt injection.
- Cross-process reasoning engine.
- Cortex UI implementation.

## Conformance Cases
- C-01 (integration): authenticated `GET /cortex/status` returns deterministic schema.
- C-02 (integration): tracked operations are reflected in event counters/recent events.
- C-03 (regression): unauthorized `GET /cortex/status` returns `401`.
- C-04 (regression): missing events artifact returns deterministic empty-state fallback payload.
- C-05 (regression): `/gateway/status` includes `cortex_status_endpoint` discovery field.
- C-06 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can inspect Cortex observer health and event activity through a dedicated endpoint.
- Key gateway operations emit Cortex observer events.
- Endpoint discovery and contract behavior are deterministic and test-backed.
