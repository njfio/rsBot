# M172 - Gateway OpenResponses Module Split Phase 2

## Objective
Continue reducing `crates/tau-gateway/src/gateway_openresponses.rs` hotspot size by extracting the gateway session endpoint domain into a dedicated runtime module while preserving contracts.

## Scope
- Phase 2 extraction target:
  - `GET /gateway/sessions`
  - `GET /gateway/sessions/{session_key}`
  - `POST /gateway/sessions/{session_key}/append`
  - `POST /gateway/sessions/{session_key}/reset`
- Preserve route constants, auth/rate-limit behavior, policy-gate enforcement, and response payload contracts.

## Linked Issues
- Epic: #2973
- Story: #2974
- Task: #2975

## Exit Criteria
- Session handlers are moved out of `gateway_openresponses.rs`.
- `gateway_openresponses.rs` line count drops below 2800.
- Targeted session endpoint tests pass.
