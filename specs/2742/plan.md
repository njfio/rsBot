# Plan: Issue #2742 - G18 priority pages baseline in embedded dashboard shell

## Approach
1. Add RED tests for required shell controls and markers.
2. Extend embedded dashboard shell HTML/JS with API endpoint constants and auth token input.
3. Implement overview/sessions/memory/configuration refresh handlers using existing endpoints.
4. Render deterministic status/output blocks and keep behavior read-only.
5. Run regression suite + live localhost smoke and update checklist evidence.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses/dashboard_shell.html`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `tasks/spacebot-comparison.md`

## Risks / Mitigations
- Risk: auth-required endpoints may fail without token context in shell.
  - Mitigation: add explicit auth token input and deterministic failure messaging.
- Risk: route contract regressions in status/webchat.
  - Mitigation: keep changes isolated to shell asset and verify full `tau-gateway` suite.

## Interfaces / Contracts
- Reuse existing endpoints:
  - `GET /dashboard/health`
  - `GET /dashboard/widgets`
  - `GET /gateway/sessions`
  - `GET /gateway/sessions/{session_key}`
  - `GET /api/memories/graph`
  - `GET /gateway/config`
- No endpoint shape changes.

## ADR
- Not required: no dependency/protocol changes.
