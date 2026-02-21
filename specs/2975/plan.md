# Plan: Issue #2975 - Session API runtime extraction

## Approach
1. Identify session route handlers and local helper functions in `gateway_openresponses.rs`.
2. Create `gateway_openresponses/session_api_runtime.rs` and move those functions with `pub(super)` visibility.
3. Update `gateway_openresponses.rs` module imports and keep route constants/registrations unchanged.
4. Run targeted session endpoint tests.
5. Validate fmt/clippy gates and file-size threshold.

## Affected Paths
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/session_api_runtime.rs` (new)
- `crates/tau-gateway/src/gateway_openresponses/tests.rs` (only if additional regression needed)

## Risks and Mitigations
- Risk: subtle behavior drift in session append/reset.
  - Mitigation: move code with no semantic changes, run targeted session tests.
- Risk: visibility/import resolution issues after split.
  - Mitigation: use `pub(super)` handlers and compile/lint gates.

## Interfaces / Contracts
- Session endpoints:
  - `GET /gateway/sessions`
  - `GET /gateway/sessions/{session_key}`
  - `POST /gateway/sessions/{session_key}/append`
  - `POST /gateway/sessions/{session_key}/reset`
- Policy gate requirement:
  - `allow_session_write` for append/reset routes

## ADR
Not required (internal module boundary refactor only).
