# Spec: Issue #2614 - Build production dashboard UI (G18) with auth and live status views

Status: Accepted

## Problem Statement
Tau's gateway already exposes dashboard/status/action/stream APIs, but the web operator UI does not yet provide a dedicated, authenticated, live-updating dashboard workflow that operators can use for overview + control in one place.

## Acceptance Criteria

### AC-1 Dashboard view is available in the gateway web UI
Given the gateway webchat page,
When an operator loads `/webchat`,
Then a dedicated Dashboard view is available with overview metrics, widget/timeline/alert panes, and action controls.

### AC-2 Dashboard data and controls use authenticated gateway endpoints
Given dashboard API auth is enabled,
When the operator refreshes dashboard data or posts dashboard actions,
Then requests use bearer-authenticated dashboard endpoints and render success/error results in-place.

### AC-3 Live status updates are supported in the dashboard view
Given dashboard live mode is enabled in the UI,
When the configured poll interval elapses,
Then the UI refreshes dashboard health/widgets/timeline/alerts without page reload and can be paused.

### AC-4 Existing dashboard API contract behavior remains intact
Given existing dashboard endpoints and stream/action contracts,
When the integration suite runs,
Then health/widgets/timeline/alerts/action/stream behavior continues to pass.

### AC-5 Scoped verification gates are green
Given this issue scope,
When formatting, linting, and targeted gateway tests run,
Then all checks pass.

## Scope

### In Scope
- Extend gateway webchat HTML/JS with a dedicated dashboard tab.
- Add authenticated dashboard fetch/control helpers in the UI.
- Add live polling toggle/interval behavior for dashboard status refresh.
- Add/adjust tests validating dashboard UI surface + endpoint integration continuity.

### Out of Scope
- New backend dashboard endpoint semantics.
- New external frontend framework migration.
- Dashboard role-based multi-principal auth redesign.

## Conformance Cases
- C-01 (unit): rendered webchat page includes dashboard tab, controls, and live status elements.
- C-02 (functional): `/webchat` HTML includes dashboard operator surface and live controls.
- C-03 (integration): dashboard snapshot endpoints return health/widgets/timeline/alerts.
- C-04 (integration): dashboard action endpoint persists control/audit updates.
- C-05 (integration): dashboard stream reconnect emits reset + snapshot events.
- C-06 (verify): `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests pass.

## Success Metrics / Observable Signals
- Operators can open a dashboard tab and see current health/training/widget/timeline/alert state.
- Pause/resume/refresh actions return visible result payloads.
- Live refresh can be enabled/disabled from the UI without full reload.
