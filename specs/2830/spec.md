# Spec: Issue #2830 - Chat message send and transcript visibility contracts

Status: Implemented

## Problem Statement
Tau Ops chat route scaffolding exists, but there is no live message-send pathway that appends operator input and re-renders transcript state deterministically in SSR output. PRD phase 1M requires end-to-end contracts for `Chat -> Message sends and appears in chat`.

## Acceptance Criteria

### AC-1 `/ops/chat` exposes deterministic send-form and transcript SSR markers
Given the Tau Ops chat shell render,
When operators inspect `/ops/chat` HTML,
Then deterministic marker contracts exist for chat form action/method/session and transcript rows.

### AC-2 Chat transcript markers map from active session state
Given an active session key for chat,
When `/ops/chat` renders,
Then transcript markers map role/content rows from persisted session lineage for that session.

### AC-3 `POST /ops/chat/send` appends user messages and redirects back to chat
Given a valid message submission to `/ops/chat/send`,
When the request is processed,
Then the message is appended to the session store and the response redirects to `/ops/chat` preserving theme/sidebar/session controls.

### AC-4 Existing Tau Ops shell contracts remain stable
Given existing phase 1A..1L ops shell suites,
When chat send/transcript integration lands,
Then prior suites remain green.

## Scope

### In Scope
- `tau-dashboard-ui` chat panel form/transcript SSR contract coverage.
- `tau-gateway` ops chat transcript hydration from `SessionStore`.
- `tau-gateway` `POST /ops/chat/send` append + redirect behavior.
- Targeted regression validation for existing ops shell slices.

### Out of Scope
- Streaming chat responses in `/ops/chat`.
- Auth session cookie model changes.
- New message editing/deletion UI controls.

## Conformance Cases
- C-01 (functional): `/ops/chat` SSR includes send-form and transcript marker contracts.
- C-02 (integration): `/ops/chat` transcript markers reflect persisted active-session messages.
- C-03 (integration): `POST /ops/chat/send` appends user message, redirects to `/ops/chat`, and message appears in transcript markers.
- C-04 (regression): existing ops shell suites (auth/nav/theme/control/timeline/alerts/connectors) remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui functional_spec_2830 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2830 -- --test-threads=1` passes.
- `cargo test -p tau-gateway integration_spec_2830 -- --test-threads=1` passes.
- `cargo test -p tau-dashboard-ui functional_spec_2826 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2802 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2826 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule. Human review required in PR.
