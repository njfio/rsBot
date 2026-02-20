# Plan: Issue #2834 - Chat active session selector contracts

## Approach
1. Add RED conformance tests for selector container/option markers in `tau-dashboard-ui`.
2. Add RED gateway tests that seed multiple session files and assert selector option rows + active selected-state on `/ops/chat`.
3. Extend `TauOpsDashboardChatSnapshot` with selector option rows and render deterministic selector markers.
4. In gateway ops shell, discover session keys from `state_dir/openresponses/sessions/*.jsonl`, sanitize/sort keys, ensure the active session key is included, and map options into the chat snapshot.
5. Run targeted regressions for existing chat shell contracts and validate crate gates.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-gateway/src/gateway_openresponses/ops_dashboard_shell.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks & Mitigations
- Risk: selector option ordering is non-deterministic due to filesystem iteration order.
  - Mitigation: normalize, deduplicate, and sort keys before rendering.
- Risk: active query session is missing from selector options when no file exists yet.
  - Mitigation: always include sanitized active session key in selector options.
- Risk: selector additions regress existing chat form/transcript contracts.
  - Mitigation: keep existing markers unchanged and run existing 2830 suites.

## Interfaces / Contracts
- `TauOpsDashboardChatSnapshot` adds `session_options`.
- New UI SSR markers:
  - `id="tau-ops-chat-session-selector"` with `data-active-session-key` and `data-option-count`.
  - `id="tau-ops-chat-session-options"` container.
  - `id="tau-ops-chat-session-option-<index>"` rows with `data-session-key` and `data-selected`.
- Gateway chat snapshot collection includes discovered session option rows from session storage.

## ADR
No ADR required: no new dependency, protocol schema, or architecture boundary change.
