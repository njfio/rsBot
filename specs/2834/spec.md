# Spec: Issue #2834 - Chat active session selector contracts

Status: Implemented

## Problem Statement
Tau Ops chat currently binds transcript and send-form state to a single active session key, but there is no deterministic SSR selector contract that surfaces available session choices for operators. PRD phase 1N requires `/ops/chat` to expose active session selection markers, render deterministic option rows for discovered sessions, and keep selection synchronized with transcript and send-form state.

## Acceptance Criteria

### AC-1 `/ops/chat` exposes deterministic session-selector SSR markers
Given the Tau Ops chat shell render,
When operators inspect `/ops/chat` HTML,
Then deterministic session-selector container markers and option-row markers are present.

### AC-2 Session selector options map discovered gateway sessions
Given session files exist in gateway openresponses session storage,
When `/ops/chat` renders,
Then selector option rows deterministically include discovered session keys and flag the active session key as selected.

### AC-3 Active session selection remains synchronized across selector, transcript, and send form
Given an active session query selection on `/ops/chat`,
When the page renders,
Then selector selected-state, transcript rows, and send-form hidden `session_key` all reference the same active session key.

### AC-4 Existing Tau Ops shell contracts remain stable
Given existing phase 1A..1M suites,
When active-session selector contracts land,
Then prior suites remain green.

## Scope

### In Scope
- `tau-dashboard-ui` chat selector snapshot contract types and SSR markers.
- `tau-gateway` chat selector option discovery from gateway session storage.
- Conformance + integration tests for active-session selector behavior.

### Out of Scope
- Client-side dynamic filtering/search for session options.
- Session metadata badges (timestamps, token counters, owners) in selector rows.
- Auth/session token model changes.

## Conformance Cases
- C-01 (functional): `/ops/chat` includes deterministic selector container + option row SSR markers.
- C-02 (integration): selector rows include discovered session keys and selected marker matches active query session.
- C-03 (integration): active selection synchronizes selector selected marker, transcript content, and send-form `session_key`.
- C-04 (regression): existing Tau Ops shell suites remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui functional_spec_2834 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2834 -- --test-threads=1` passes.
- `cargo test -p tau-gateway integration_spec_2834 -- --test-threads=1` passes.
- `cargo test -p tau-dashboard-ui functional_spec_2830 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2830 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule. Human review required in PR.
