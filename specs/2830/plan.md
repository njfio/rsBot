# Plan: Issue #2830 - Chat message send and transcript visibility contracts

## Approach
1. Add/extend RED conformance tests for `/ops/chat` send-form markers and message visibility contracts in both UI and gateway layers.
2. Extend `tau-dashboard-ui` shell tests to assert deterministic chat form/transcript markers.
3. In `tau-gateway`, hydrate chat snapshot rows from active session lineage using query controls (`session`/`session_key`).
4. Add `POST /ops/chat/send` to append user messages and redirect to `/ops/chat` with preserved `theme`/`sidebar`/`session` controls.
5. Run targeted regressions for existing ops shell slices and validate crate gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_shell_controls.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: chat transcript hydration leaks system prompt rows and clutters transcript contracts.
  - Mitigation: filter system rows and blank message payloads from rendered chat rows.
- Risk: send endpoint drops operator shell controls after redirect.
  - Mitigation: deterministic redirect builder carries validated `theme`, `sidebar`, and sanitized `session` query tokens.
- Risk: control query expansion (`session`/`session_key`) regresses existing route behavior.
  - Mitigation: add unit coverage for control parsing + keep existing default behavior unchanged.

## Interfaces / Contracts
- New gateway route: `POST /ops/chat/send` (form payload: `session_key`, `message`, `theme`, `sidebar`).
- Existing route update: `GET /ops/chat` reads `session`/`session_key` query token and maps active session transcript rows.
- UI shell contracts:
  - `id="tau-ops-chat-send-form"` with deterministic `action`, `method`, and session/theme/sidebar hidden inputs.
  - `id="tau-ops-chat-transcript"` with deterministic `data-message-count` and row markers.

## ADR
No ADR required: no new dependency, protocol schema, or architecture boundary change.
