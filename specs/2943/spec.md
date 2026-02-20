# Spec: Issue #2943 - Real-time stream connection markers and conformance tests

Status: Implemented

## Problem Statement
Tau Ops PRD real-time acceptance items (`2147-2152`) require explicit, testable stream lifecycle contracts, but the current Leptos shell does not expose a dedicated stream-connection contract surface linking connection behavior, heartbeat updates, alert pushes, chat token streaming, connector-health updates, and reconnect backoff behavior.

## Acceptance Criteria

### AC-1 Stream transport connects on app load
Given the Tau Ops shell is rendered,
When stream bootstrap markers are inspected,
Then output declares websocket transport and connect-on-load behavior.

### AC-2 Heartbeat, alert, and connector targets are declared for real-time updates
Given stream contract markers,
When heartbeat and dashboard stream bindings are inspected,
Then stat-card, alert-feed, and connector-health targets are declared.

### AC-3 Chat token streaming contract declares no-polling mode
Given stream contract markers,
When chat streaming behavior is inspected,
Then output declares token-stream updates as stream-driven (not polling).

### AC-4 Reconnect strategy contracts are declared
Given stream contract markers,
When disconnect handling is inspected,
Then output declares reconnect/backoff strategy markers.

## Scope

### In Scope
- Add deterministic stream contract marker section in `tau-dashboard-ui` SSR shell.
- Add conformance tests for PRD items `2147-2152`.
- Keep existing route/panel marker behavior stable.

### Out of Scope
- Live websocket runtime implementation.
- Real network transport, retries, or browser-side event loop code.
- Backend stream endpoints.

## Conformance Cases
- C-01: stream section declares websocket and connect-on-load.
- C-02: heartbeat target marker points to KPI/stat-card surface.
- C-03: alert stream target marker points to alert feed.
- C-04: chat stream mode declares no-polling token stream.
- C-05: connector-health stream target marker points to connector table.
- C-06: reconnect/backoff strategy markers are present.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui -- --test-threads=1` passes with new `spec_c0x` tests.
- Existing dashboard contract tests remain green.

## Approval Gate
P1 single-module scope. Spec is agent-reviewed and proceeds under the user’s explicit instruction to continue contract execution end-to-end.
