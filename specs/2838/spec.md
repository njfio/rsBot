# Spec: Issue #2838 - Sessions explorer deterministic row contracts

Status: Implemented

## Problem Statement
Tau Ops already exposes `/ops/sessions` as a route token, but there is no dedicated sessions explorer surface with deterministic row-level SSR markers for discovered gateway sessions. Phase 1O requires explicit contracts so operators can inspect available sessions and navigate to chat with preserved shell controls.

## Acceptance Criteria

### AC-1 `/ops/sessions` exposes deterministic panel and list markers
Given a request to `/ops/sessions`,
When operators inspect SSR HTML,
Then deterministic sessions panel and list container markers are present.

### AC-2 Session rows map discovered gateway session files deterministically
Given one or more session files in gateway openresponses storage,
When `/ops/sessions` renders,
Then deterministic row markers include sanitized session keys and stable ordering.

### AC-3 Session rows expose deterministic chat-route links with preserved controls
Given theme/sidebar/session controls in `/ops/sessions`,
When row links render,
Then each row contains deterministic `/ops/chat` href markers preserving `theme`, `sidebar`, and `session` query state.

### AC-4 Empty-state marker renders when no session files exist
Given no session files in gateway storage,
When `/ops/sessions` renders,
Then an explicit deterministic empty-state marker is rendered.

### AC-5 Existing Tau Ops shell contracts remain stable
Given existing phase 1A..1N suites,
When sessions explorer contracts land,
Then prior suites remain green.

## Scope

### In Scope
- `tau-dashboard-ui` sessions panel/list/row/empty-state SSR markers.
- `tau-gateway` sessions explorer snapshot mapping from session file discovery.
- Conformance + integration tests for `/ops/sessions`.

### Out of Scope
- Session deletion/reset actions from `/ops/sessions`.
- Pagination/filter/search controls for sessions explorer.
- Client-side hydration behaviors beyond SSR marker contracts.

## Conformance Cases
- C-01 (functional): `/ops/sessions` exposes deterministic panel/list markers.
- C-02 (integration): session rows map discovered session files with stable markers.
- C-03 (integration): row href markers preserve `theme`/`sidebar`/`session` controls for `/ops/chat`.
- C-04 (functional): empty-state marker renders when no session files exist.
- C-05 (regression): existing ops-shell and chat suites remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui functional_spec_2838 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2838 -- --test-threads=1` passes.
- `cargo test -p tau-gateway integration_spec_2838 -- --test-threads=1` passes.
- `cargo test -p tau-dashboard-ui functional_spec_2834 -- --test-threads=1` passes.
- `cargo test -p tau-gateway spec_2834 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule. Human review required in PR.
