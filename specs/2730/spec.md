# Spec: Issue #2730 - G18 stretch Cortex admin chat webchat panel

Status: Implemented

## Problem Statement
Tau ships authenticated Cortex admin endpoints, but operators still need manual API tooling to use them. G18 stretch parity requires a Cortex admin chat surface directly in gateway webchat so runtime observer interactions are available in the operational UI.

## Acceptance Criteria

### AC-1 Webchat exposes a Cortex admin view with operator controls
Given the gateway webchat page,
When it renders tabs/views,
Then a dedicated Cortex admin view provides prompt input, send action, and structured output/status panes.

### AC-2 Cortex requests stream from webchat using authenticated `/cortex/chat`
Given a valid auth token and prompt input,
When the operator submits Cortex admin prompt,
Then webchat sends `POST /cortex/chat` and consumes SSE frames (`created`, `delta`, `done`) into the Cortex output pane.

### AC-3 Cortex failure states produce deterministic diagnostics
Given unauthorized or malformed Cortex responses,
When Cortex panel request handling fails,
Then Cortex status/output panes show deterministic failure messages and telemetry reason codes.

### AC-4 Existing webchat views remain compatible
Given conversation/dashboard/sessions/memory/configuration flows,
When Cortex panel is added,
Then existing webchat controls and regressions remain green.

### AC-5 Scoped verification gates pass
Given this slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted gateway tests for webchat+cortex interactions pass.

## Scope

### In Scope
- Webchat HTML/JS updates for Cortex admin panel.
- SSE frame handling for Cortex event names in UI script.
- Tests covering Cortex webchat panel markup/script contracts.
- Update `tasks/spacebot-comparison.md` G18 stretch evidence where applicable.

### Out of Scope
- New frontend framework migration.
- Changes to Cortex backend endpoint contracts.
- Cron management UI implementation.

## Conformance Cases
- C-01 (unit): webchat HTML contains Cortex tab/view controls and endpoint wiring markers.
- C-02 (unit): Cortex script path parses/handles `cortex.response.created`, `cortex.response.output_text.delta`, and `cortex.response.output_text.done` events.
- C-03 (functional): Cortex prompt submission path calls `/cortex/chat` with auth headers and status handling markers.
- C-04 (regression): existing webchat endpoint shell/memory graph tests and cortex endpoint API tests remain green.
- C-05 (verify): fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Operators can execute Cortex chat interactions entirely from `/webchat`.
- Cortex stream visibility is available without external SSE tooling.
- Existing gateway webchat behaviors remain stable.
